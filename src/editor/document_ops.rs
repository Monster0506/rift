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
        self.document_manager
            .active_document()
            .map(pred)
            .unwrap_or(false)
    }

    pub(super) fn reject_read_only_edit(&mut self) {
        self.state.notify(
            crate::notification::NotificationType::Warning,
            "Cannot make changes: buffer is read-only".to_string(),
        );
    }

    pub fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        self.remove_document_with_intent(id, crate::document::RemovalIntent::Normal)
    }

    pub(super) fn remove_document_force(&mut self, id: DocumentId) -> Result<(), RiftError> {
        self.remove_document_with_intent(id, crate::document::RemovalIntent::Force)
    }

    pub(super) fn remove_private_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        if self.document_manager.get_document(id).is_none() {
            return Ok(());
        }
        self.remove_document_with_intent(id, crate::document::RemovalIntent::Force)
    }

    fn remove_document_with_intent(
        &mut self,
        id: DocumentId,
        intent: crate::document::RemovalIntent,
    ) -> Result<(), RiftError> {
        let plan = self.document_manager.prepare_removal(id, intent)?;
        let (handle, kind_id, descriptor) = {
            let document = self.document_manager.get_document(id).ok_or_else(|| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    format!("Document {id} not found"),
                )
            })?;
            (
                document.handle(),
                document.buffer_kind_id(),
                std::sync::Arc::clone(&document.kind.descriptor),
            )
        };

        self.job_manager.cancel_jobs_for_handle(handle);
        match descriptor.on_close {
            crate::document::CloseHandler::Native(close) => close(handle),
            crate::document::CloseHandler::Lua => {
                self.plugin_host.invoke_buffer_close(id, descriptor.name());
            }
        }
        self.apply_plugin_mutations();

        #[cfg(feature = "lsp")]
        {
            self.lsp_notify_close(id);
            if self
                .pending_goto_target
                .map(|(d, ..)| d == id)
                .unwrap_or(false)
            {
                self.pending_goto_target = None;
            }
        }

        self.file_load_jobs.retain(|_, target_id| *target_id != id);
        self.clear_git_gutter_state(id);
        self.pending_syntax_reparse.remove(&id);
        self.pending_git_status_expand_all.remove(&id);
        self.pending_git_log_expand_head.remove(&id);
        self.display_map_cache.retain(|entry| entry.doc_id != id);
        if self.pending_text_changed == Some(id) {
            self.pending_text_changed = None;
        }
        if self
            .pending_cursor_moved
            .map(|(d, ..)| d == id)
            .unwrap_or(false)
        {
            self.pending_cursor_moved = None;
        }
        if self
            .search_highlights_synced
            .as_ref()
            .map(|(d, ..)| *d == id)
            .unwrap_or(false)
        {
            self.search_highlights_synced = None;
        }

        let has_replacement = plan.has_replacement();
        self.document_manager.commit_removal(plan);
        self.buffer_kinds.decrement_open_count(kind_id);
        if has_replacement {
            self.buffer_kinds
                .increment_open_count(crate::document::BufferKindId::FILE);
        }
        if let Some(doc_id) = self.document_manager.active_document_id() {
            for win_id in self.split_tree.windows_for_document(id) {
                self.split_tree.set_window_document(win_id, doc_id);
            }
            self.split_tree.set_focused_document(doc_id);
        }
        self.sync_state_with_active_document();

        self.update_lua_state();
        self.plugin_host
            .dispatch(&crate::plugin::EditorEvent::BufClose { buf: id });
        self.apply_plugin_mutations();

        Ok(())
    }

    fn resolve_link_path(&self, path_str: String) -> String {
        let base = self
            .document_manager
            .active_document()
            .and_then(|d| d.path())
            .and_then(|p| p.parent().map(|x| x.to_path_buf()));
        resolve_link_path_in(path_str, base.as_deref())
    }

    pub fn open_file(&mut self, file_path: Option<String>, force: bool) -> Result<(), RiftError> {
        if let Some(path_str) = file_path {
            let path_str = self.resolve_link_path(path_str);
            let path = std::path::PathBuf::from(&path_str);
            if self
                .document_manager
                .find_open_document_index(&path)
                .is_some()
            {
                self.save_current_view_state();
                self.document_manager.open_file(Some(path_str), force)?;
                self.restore_view_state();
            } else if path.exists() {
                self.save_current_view_state();
                let id = self.document_manager.next_id();
                match crate::document::Document::try_open_undo_file(id, &path) {
                    Some(Ok(doc)) => {
                        self.document_manager.add_document(doc);
                    }
                    Some(Err(e)) => return Err(e),
                    None => {
                        let doc_id = self.document_manager.create_placeholder(&path_str)?;
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
                let doc_id = self.document_manager.create_placeholder(&path_str)?;
                #[cfg(feature = "lsp")]
                self.lsp_notify_open(doc_id);
                #[cfg(not(feature = "lsp"))]
                let _ = doc_id;
            }
        } else if let Some(doc) = self.document_manager.active_document() {
            if doc.buffer_kind_id() == crate::document::BufferKindId::FILE {
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
                    doc.id,
                    path.to_path_buf(),
                );
                let doc_id = doc.id;
                let job_id = self.job_manager.spawn(job);
                self.file_load_jobs.insert(job_id, doc_id);
            } else {
                let kind_id = doc.buffer_kind_id();
                let reload_dispatch = doc.descriptor().reload_dispatch;
                match reload_dispatch {
                    crate::document::ReloadDispatch::Native(_) => {
                        let handler = self.native_reload_handlers.get(&kind_id).copied();
                        match handler {
                            Some(handler) => handler(self)?,
                            None => {
                                return Err(RiftError::new(
                                    ErrorType::Execution,
                                    crate::constants::errors::RELOAD_UNSUPPORTED,
                                    crate::constants::errors::MSG_RELOAD_UNSUPPORTED,
                                ))
                            }
                        }
                    }
                    crate::document::ReloadDispatch::Unsupported => {
                        return Err(RiftError::new(
                            ErrorType::Execution,
                            crate::constants::errors::RELOAD_UNSUPPORTED,
                            crate::constants::errors::MSG_RELOAD_UNSUPPORTED,
                        ));
                    }
                }
            }
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

    pub fn create_scratch_buffer(
        &mut self,
        title: String,
        lines: &[String],
    ) -> Result<crate::document::DocumentId, RiftError> {
        let id = self.document_manager.next_id();
        let doc = crate::document::Document::new_scratch(id, title, lines)?;
        self.document_manager.add_document(doc);
        self.document_manager.switch_to_document(id)?;
        self.split_tree.set_focused_document(id);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
        self.plugin_host
            .dispatch(&crate::plugin::EditorEvent::BufOpen {
                buf: id,
                path: None,
                filetype: None,
            });
        Ok(id)
    }

    pub fn open_terminal(&mut self, shell_cmd: Option<String>) -> Result<(), RiftError> {
        let size = self
            .term
            .get_size()
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERM_SIZE", e))?;

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
            .map(|l| l.cols)
            .unwrap_or(size.cols as usize);
        let (doc, rx) = crate::document::Document::new_terminal(
            id,
            terminal_rows as u16,
            terminal_cols as u16,
            shell_cmd,
        )?;

        self.document_manager.add_document(doc);

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
                crate::document::BufferKindId::TERMINAL,
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
                doc.buffer.clear_desired_col();
                let _ = doc.buffer.set_cursor(m.range.start);
                true
            }
            Ok((None, _stats)) => false,
            Err(e) => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("Search error: {}", e),
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
