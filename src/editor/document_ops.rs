use super::Editor;
use crate::action::{Action, EditorAction};
use crate::error::{ErrorType, RiftError};
use crate::job_manager::jobs::file_operations::FileLoadMode;
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
            .get_document(self.active_document_id())
            .is_some_and(pred)
    }

    fn resolve_link_path(&self, path_str: String) -> String {
        let base = self
            .document_manager
            .get_document(self.active_document_id())
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
        let Some(path_str) = file_path else {
            let buf = self.active_document_id();
            self.reload_document(buf, force)?;
            return self.focus_document(buf);
        };
        let path_str = self.resolve_link_path(path_str);
        let path = std::path::PathBuf::from(&path_str);
        if path.is_dir() {
            return Err(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::NOT_A_FILE,
                crate::constants::errors::MSG_NOT_A_FILE,
            ));
        }
        let target = if let Some(existing) = self.document_manager.find_open_document_id(&path) {
            existing
        } else if path.exists() {
            let id = self.document_manager.next_id();
            match crate::document::Document::try_open_undo_file(id, &path) {
                Some(Ok(doc)) => {
                    self.open_buffer(doc, crate::document::manager::Placement::Background)
                }
                Some(Err(e)) => return Err(e),
                None => {
                    let doc_id = self.open_placeholder(&path_str)?;
                    self.start_file_load(doc_id, path, FileLoadMode::Open);
                    doc_id
                }
            }
        } else if crate::document::manager::parent_dir_missing(&path) {
            return Err(RiftError::new(
                ErrorType::Io,
                crate::constants::errors::PARENT_DIR_MISSING,
                crate::constants::errors::MSG_PARENT_DIR_MISSING,
            ));
        } else {
            let doc_id = self.open_placeholder(&path_str)?;
            self.announce_buffer(doc_id);
            doc_id
        };
        self.focus_document(target)
    }

    /// # Errors
    /// Returns `Err` when the scratch document cannot be created or switched to.
    #[cfg(any(feature = "plugins", test))]
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
        self.open_buffer(draft, crate::document::manager::Placement::Background);
        self.focus_document(id)?;
        let _ = self.force_full_redraw();
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

        self.open_buffer(doc, crate::document::manager::Placement::Background);

        self.focus_document(id)?;
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
