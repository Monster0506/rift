use super::file_ops::NativeReloadHandler;
use super::Editor;
use crate::action::{Action, EditorAction};
use crate::document::DocumentId;
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

fn resolve_link_path_in(path_str: String, base_dir: Option<&std::path::Path>) -> String {
    let p = std::path::Path::new(&path_str);
    if p.is_absolute() || p.exists() {
        return path_str;
    }
    if let Some(dir) = base_dir {
        let candidate = dir.join(p);
        if candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    path_str
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn active_doc_is(&self, pred: impl Fn(&crate::document::Document) -> bool) -> bool {
        self.document_manager.active_document().is_some_and(pred)
    }

    pub(super) fn reject_read_only_edit(&mut self) {
        self.state.notify(
            crate::notification::NotificationType::Warning,
            "Cannot make changes: buffer is read-only".to_string(),
        );
    }

    fn resolve_link_path(&self, path_str: String) -> String {
        let base = self
            .document_manager
            .active_document()
            .and_then(|d| d.path())
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        resolve_link_path_in(path_str, base.as_deref())
    }

    /// # Errors
    /// Returns `Err` when an undo file cannot be opened, a placeholder cannot be created,
    /// the parent directory is missing, or the active document cannot be reloaded
    /// (unsaved changes without `force`, or reload unsupported).
    pub(super) fn open_file(
        &mut self,
        file_path: Option<String>,
        force: bool,
    ) -> Result<(), RiftError> {
        if let Some(path_str) = file_path {
            let path_str = self.resolve_link_path(path_str);
            let path = std::path::PathBuf::from(&path_str);
            if let Some(existing) = self.document_manager.find_open_document_id(&path) {
                self.save_current_view_state();
                self.document_manager.switch_to_document(existing)?;
                self.restore_view_state();
            } else if path.exists() {
                self.save_current_view_state();
                let id = self.document_manager.next_id();
                match crate::document::Document::try_open_undo_file(id, &path) {
                    Some(Ok(doc)) => {
                        self.open_buffer(doc, crate::document::manager::Placement::Focused);
                    }
                    Some(Err(e)) => return Err(e),
                    None => {
                        let doc_id = self.open_placeholder(&path_str)?;
                        let job = crate::job_manager::jobs::file_operations::FileLoadJob::new(
                            doc_id,
                            path.clone(),
                        );
                        let job_id = self.job_manager.spawn(job);
                        self.file_load_jobs.insert(job_id, doc_id);
                    }
                }
            } else if crate::document::manager::parent_dir_missing(&path) {
                return Err(RiftError::new(
                    ErrorType::Io,
                    crate::constants::errors::PARENT_DIR_MISSING,
                    crate::constants::errors::MSG_PARENT_DIR_MISSING,
                ));
            } else {
                self.save_current_view_state();
                let doc_id = self.open_placeholder(&path_str)?;
                #[cfg(feature = "lsp")]
                self.lsp_notify_open(doc_id);
                #[cfg(not(feature = "lsp"))]
                let _ = doc_id;
            }
        } else if let Some(buf) = self.document_manager.active_document_id() {
            self.reload_document(buf, force)?;
        } else {
            return Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                "No active document",
            ));
        }

        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.set_focused_document(doc_id);
        }
        self.sync_state_with_active_document();
        Ok(())
    }

    pub(super) fn with_document_target<R>(
        &mut self,
        buf: DocumentId,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let win = self.split_tree.focused_window_id();
        let original = self.active_document_id();
        let original_tab = self.document_manager.active_document_id();
        let scroll = self.render_system.viewport.get_scroll();
        self.split_tree.set_window_document(win, buf);
        let _ = self.document_manager.switch_to_document(buf);
        self.sync_state_with_active_document();
        let result = f(self);
        if original != buf && self.document_manager.get_document(original).is_some() {
            if self
                .split_tree
                .get_window(win)
                .is_some_and(|w| w.document_id == buf)
            {
                self.split_tree.set_window_document(win, original);
                if self.split_tree.focused_window_id() == win {
                    self.render_system.viewport.set_scroll(scroll.0, scroll.1);
                }
            }
            if self.document_manager.active_document_id() == Some(buf) {
                if let Some(tab) = original_tab {
                    let _ = self.document_manager.switch_to_document(tab);
                }
            }
            self.sync_state_with_active_document();
        }
        result
    }

    pub(super) fn reload_document(
        &mut self,
        buf: DocumentId,
        force: bool,
    ) -> Result<(), RiftError> {
        let Some(doc) = self.document_manager.get_document(buf) else {
            return Ok(());
        };
        if doc.buffer_kind_id() == crate::document::runtime::BufferKindId::FILE {
            let Some(path) = doc.path() else {
                return Err(RiftError::new(
                    ErrorType::Execution,
                    crate::constants::errors::NO_PATH,
                    "No file name",
                ));
            };
            if !force && doc.is_dirty() {
                return Err(RiftError {
                    severity: ErrorSeverity::Warning,
                    kind: ErrorType::Execution,
                    code: crate::constants::errors::UNSAVED_CHANGES.to_string(),
                    message: crate::constants::errors::MSG_UNSAVED_CHANGES.to_string(),
                });
            }
            let job = crate::job_manager::jobs::file_operations::FileLoadJob::new_reload(
                buf,
                path.to_path_buf(),
            );
            let job_id = self.job_manager.spawn(job);
            self.file_load_jobs.insert(job_id, buf);
        } else {
            let kind_id = doc.buffer_kind_id();
            let dispatch = doc.descriptor().reload_dispatch;
            self.with_document_target(buf, |editor| {
                editor.reload_native_document(kind_id, dispatch)
            })?;
        }
        Ok(())
    }

    fn reload_native_document(
        &mut self,
        kind_id: crate::document::runtime::BufferKindId,
        reload_dispatch: crate::document::runtime::ReloadDispatch,
    ) -> Result<(), RiftError> {
        match reload_dispatch {
            crate::document::runtime::ReloadDispatch::Native => {
                let handler = self.native_reload_handlers.get(&kind_id).copied();
                match handler {
                    Some(NativeReloadHandler::Infallible(handler)) => handler(self),
                    Some(NativeReloadHandler::Fallible(handler)) => handler(self)?,
                    None => {
                        return Err(RiftError::new(
                            ErrorType::Execution,
                            crate::constants::errors::RELOAD_UNSUPPORTED,
                            crate::constants::errors::MSG_RELOAD_UNSUPPORTED,
                        ))
                    }
                }
            }
            crate::document::runtime::ReloadDispatch::Unsupported => {
                return Err(RiftError::new(
                    ErrorType::Execution,
                    crate::constants::errors::RELOAD_UNSUPPORTED,
                    crate::constants::errors::MSG_RELOAD_UNSUPPORTED,
                ));
            }
        }
        Ok(())
    }

    /// # Errors
    /// Returns `Err` when the scratch document cannot be created or switched to.
    pub(super) fn create_scratch_buffer(
        &mut self,
        reservation: crate::document::manager::CreationReservation,
        title: String,
        lines: &[String],
    ) -> Result<crate::document::DocumentId, RiftError> {
        let id = reservation.id();
        let mut doc = crate::document::Document::new_scratch(id, title, lines);
        doc.set_handle(reservation.handle());
        let draft = crate::document::manager::DocumentDraft::new(reservation, doc)?;
        self.open_buffer(draft, crate::document::manager::Placement::Focused);
        self.split_tree.set_focused_document(id);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
        self.call_plugins(|editor| {
            editor
                .plugin_host
                .dispatch(&crate::plugin::events::EditorEvent::BufOpen {
                    buf: id,
                    path: None,
                    filetype: None,
                });
        });
        Ok(id)
    }

    /// # Errors
    /// Returns `Err` when the terminal size cannot be read or the terminal document
    /// cannot be created.
    /// # Panics
    /// Panics if the new terminal document has no handle (`terminal_emulation` builds).
    pub(super) fn open_terminal(&mut self, shell_cmd: Option<String>) -> Result<(), RiftError> {
        let size = self
            .term
            .get_size()
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERM_SIZE", e.to_string()))?;

        let id = self.document_manager.next_id();
        let terminal_rows = (size.rows as usize).saturating_sub(1).max(1);
        let content_rows = (size.rows as usize).saturating_sub(1);
        let layouts = self
            .split_tree
            .compute_layout(content_rows, size.cols as usize);
        let focused_id = self.split_tree.focused_window_id();
        let terminal_cols = layouts
            .iter()
            .find(|l| l.window_id == focused_id)
            .map_or(size.cols as usize, |l| l.cols);
        let (doc, rx) = crate::document::Document::new_terminal(
            id,
            u16::try_from(terminal_rows).unwrap_or(u16::MAX),
            u16::try_from(terminal_cols).unwrap_or(u16::MAX),
            shell_cmd,
        )?;

        self.open_buffer(doc, crate::document::manager::Placement::Focused);

        self.document_manager.switch_to_document(id)?;
        self.split_tree.set_focused_document(id);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();

        #[cfg(feature = "terminal_emulation")]
        {
            let handle = self
                .document_manager
                .get_handle(id)
                .expect("new terminal document has a handle");
            let token = self.job_manager.next_token(
                handle,
                crate::document::runtime::BufferKindId::TERMINAL,
                crate::job_manager::AsyncOpDomain::Terminal,
            );
            let job = crate::job_manager::jobs::terminal_job::TerminalInputJob::new(token, rx);
            self.job_manager.spawn(job);
        }
        #[cfg(not(feature = "terminal_emulation"))]
        let _ = rx;

        Ok(())
    }

    pub(super) fn perform_search(
        &mut self,
        query: &str,
        direction: SearchDirection,
        skip_current: bool,
    ) -> bool {
        self.update_search_highlights();
        let _ = self.force_full_redraw();

        let doc = self
            .document_manager
            .active_document_mut()
            .expect("No active document");
        match doc.perform_search(query, direction, skip_current) {
            Ok((Some(m), _stats)) => {
                let _ = doc.set_cursor(m.range.start, crate::document::ops::CursorCause::Jump);
                true
            }
            Ok((None, _stats)) => false,
            Err(e) => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("Search error: {e}"),
                );
                false
            }
        }
    }

    pub fn goto_line(&mut self, line: usize) {
        self.handle_action(&Action::Editor(EditorAction::GotoLine(line)));
    }

    pub fn run_command(&mut self, cmd: String) {
        self.handle_action(&Action::Editor(EditorAction::RunCommand(cmd)));
    }

    pub fn jump_to_pattern(&mut self, pattern: &str) {
        self.handle_action(&Action::Editor(EditorAction::Search(pattern.to_string())));
    }
}

#[cfg(test)]
#[path = "document_ops_tests.rs"]
mod document_ops_tests;
