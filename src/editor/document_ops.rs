use super::Editor;
use crate::action::{Action, EditorAction};
use crate::document::DocumentId;
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::search::SearchDirection;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

/// Rebase a relative target onto `base_dir` only when it does not already resolve
/// against the cwd; absolute paths and unresolved targets are returned unchanged.
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
    pub fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        self.document_manager.remove_document(id)?;
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.sync_state_with_active_document();
        Ok(())
    }

    /// Resolve a relative target against the active document's directory when it
    /// does not already resolve against the cwd, so links open next to their file.
    fn resolve_link_path(&self, path_str: String) -> String {
        let base = self
            .document_manager
            .active_document()
            .and_then(|d| d.path())
            .and_then(|p| p.parent().map(|x| x.to_path_buf()));
        resolve_link_path_in(path_str, base.as_deref())
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
            let path_str = self.resolve_link_path(path_str);
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

    /// Create an in-memory scratch buffer with `lines` as its content and switch to it.
    pub fn create_scratch_buffer(
        &mut self,
        title: String,
        lines: &[String],
    ) -> Result<crate::document::DocumentId, RiftError> {
        let id = self.document_manager.next_id();
        let doc = crate::document::Document::new_scratch(id, title, lines)?;
        self.document_manager.add_document(doc);
        self.document_manager.switch_to_document(id)?;
        self.split_tree.focused_window_mut().document_id = id;
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

    /// Open a new terminal buffer
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
                doc.buffer.clear_desired_col();
                let _ = doc.buffer.set_cursor(m.range.start);
                true
            }
            Ok((None, _stats)) => false,
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

#[cfg(test)]
mod tests {
    use super::resolve_link_path_in;

    #[test]
    fn resolve_link_rebases_onto_document_dir() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("sibling_note_xyz.md");
        std::fs::write(&target, "x").unwrap();

        // A relative target that does not exist in the cwd resolves against the dir.
        let got = resolve_link_path_in("sibling_note_xyz.md".to_string(), Some(dir.path()));
        assert_eq!(got, target.to_string_lossy());

        // A target that resolves against neither is returned unchanged.
        assert_eq!(
            resolve_link_path_in("missing_zzz.md".to_string(), Some(dir.path())),
            "missing_zzz.md"
        );

        // An absolute path is never rebased.
        let abs = target.to_string_lossy().into_owned();
        assert_eq!(resolve_link_path_in(abs.clone(), Some(dir.path())), abs);

        // With no document directory, the target is left as-is.
        assert_eq!(
            resolve_link_path_in("sibling_note_xyz.md".to_string(), None),
            "sibling_note_xyz.md"
        );
    }
}
