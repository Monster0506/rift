#[allow(unused_imports)]
use crate::term::TerminalBackend;
use super::Editor;
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::action::{Action, EditorAction};
use crate::document::DocumentId;
use crate::search::SearchDirection;

impl<T: TerminalBackend> Editor<T> {
    pub fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        self.document_manager.remove_document(id)?;
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.sync_state_with_active_document();
        Ok(())
    }

    /// Open a file in a new document or reload the current one
    ///
    /// If file_path is Some, it opens that file (or creates a new document for
    /// it if not found). If file_path is None, it reloads the current active
    /// document.
    pub fn open_file(&mut self, file_path: Option<String>, force: bool) -> Result<(), RiftError> {
        // Logic split: if path provided, check if open. If not open, async load.
        // If path provided and open, switch to it (via manager).
        // If no path, reload active (async).

        if let Some(path_str) = file_path {
            let path = std::path::PathBuf::from(&path_str);
            if self
                .document_manager
                .find_open_document_index(&path)
                .is_some()
            {
                // Save current document's view state before switching
                self.save_current_view_state();
                // Already open, use manager to switch
                self.document_manager.open_file(Some(path_str), force)?;
                // Restore the switched-to document's view state
                self.restore_view_state();
            } else {
                // Save current document's view state before switching
                self.save_current_view_state();
                // Not open, create placeholder and async load
                let doc_id = self.document_manager.create_placeholder(&path_str)?;
                let job = crate::job_manager::jobs::file_operations::FileLoadJob::new(
                    doc_id,
                    path.clone(),
                );
                self.job_manager.spawn(job);
            }
        } else {
            // Reload current
            if let Some(doc) = self.document_manager.active_document() {
                if let Some(path) = doc.path() {
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
                    self.job_manager.spawn(job);
                } else {
                    return Err(RiftError::new(
                        ErrorType::Execution,
                        crate::constants::errors::NO_PATH,
                        "No file name",
                    ));
                }
            } else {
                return Err(RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    "No active document",
                ));
            }
        }

        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.sync_state_with_active_document();
        Ok(())
    }

    /// Open a new terminal buffer
    pub fn open_terminal(&mut self, shell_cmd: Option<String>) -> Result<(), RiftError> {
        let size = self
            .term
            .get_size()
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERM_SIZE", e))?;

        let id = self.document_manager.next_id();
        let terminal_rows = size.rows.saturating_sub(1).max(1); // exclude status bar row
        let (doc, rx) =
            crate::document::Document::new_terminal(id, terminal_rows, size.cols, shell_cmd)?;

        self.document_manager.add_document(doc);

        self.document_manager.switch_to_document(id)?;
        self.split_tree.focused_window_mut().document_id = id;

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();

        let job = crate::job_manager::jobs::terminal_job::TerminalInputJob {
            document_id: id,
            rx,
        };
        self.job_manager.spawn(job);

        Ok(())
    }

    /// Perform a search in the document
    pub(super) fn perform_search(
        &mut self,
        query: &str,
        direction: SearchDirection,
        skip_current: bool,
    ) -> bool {
        // Find all matches first to populate state for highlighting
        self.update_search_highlights();
        let _ = self.force_full_redraw();

        let doc = self
            .document_manager
            .active_document_mut()
            .expect("No active document");
        match doc.perform_search(query, direction, skip_current) {
            Ok((Some(m), _stats)) => {
                // Move cursor to start of match
                let _ = doc.buffer.set_cursor(m.range.start);
                true
            }
            Ok((None, _stats)) => {
                // No match found - don't move cursor, no notification needed
                // The user can see from the cursor position that nothing was found
                false
            }
            Err(e) => {
                // Actual search error (e.g., regex compilation failure)
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("Search error: {}", e),
                );
                false
            }
        }
    }

    /// Jump to a 1-indexed line. 0 means last line.
    pub fn goto_line(&mut self, line: usize) {
        self.handle_action(&Action::Editor(EditorAction::GotoLine(line)));
    }

    /// Run an ex command string (e.g. `"set wrap"`).
    pub fn run_command(&mut self, cmd: String) {
        self.handle_action(&Action::Editor(EditorAction::RunCommand(cmd)));
    }

    /// Search for a pattern and jump to the first match.
    pub fn jump_to_pattern(&mut self, pattern: &str) {
        self.handle_action(&Action::Editor(EditorAction::Search(pattern.to_string())));
    }
}
