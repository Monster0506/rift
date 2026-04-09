#[allow(unused_imports)]
use crate::term::TerminalBackend;
use super::Editor;
use crate::error::{ErrorType, RiftError};
use crate::mode::Mode;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn do_undo(&mut self, count: Option<u64>) {
        let doc = self.document_manager.active_document_mut().unwrap();
        let count = count.unwrap_or(1) as usize;
        let mut undone = 0;
        for _ in 0..count {
            if doc.undo() {
                undone += 1;
            } else {
                break;
            }
        }
        if undone == 0 {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "Already at oldest change".to_string(),
            );
        }
        self.state.clear_command_line();
        self.update_search_highlights();
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.spawn_syntax_parse_job(doc_id);
        }
    }

    pub(super) fn do_redo(&mut self, count: Option<u64>) {
        let doc = self.document_manager.active_document_mut().unwrap();
        let count = count.unwrap_or(1) as usize;
        let mut redone = 0;
        for _ in 0..count {
            if doc.redo() {
                redone += 1;
            } else {
                break;
            }
        }
        if redone == 0 {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "Already at newest change".to_string(),
            );
        }
        self.state.clear_command_line();
        self.update_search_highlights();
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.spawn_syntax_parse_job(doc_id);
        }
    }

    pub(super) fn do_undo_goto(&mut self, seq: u64) {
        let doc = self.document_manager.active_document_mut().unwrap();
        match doc.goto_seq(seq) {
            Ok(()) => {
                self.state.notify(
                    crate::notification::NotificationType::Info,
                    format!("Jumped to edit #{}", seq),
                );
                if let Some(doc_id) = self.document_manager.active_document_id() {
                    self.spawn_syntax_parse_job(doc_id);
                }
            }
            Err(e) => {
                self.state.handle_error(RiftError::new(
                    ErrorType::Execution,
                    "UNDO_ERROR",
                    e.to_string(),
                ));
            }
        }
        self.state.clear_command_line();
        self.update_search_highlights();
    }

    /// Navigate to previous (older) history entry
    pub(super) fn navigate_history_up(&mut self) {
        let current_line = self.state.command_line.clone();
        let history = if self.current_mode == Mode::Command {
            &mut self.state.command_history
        } else if self.current_mode == Mode::Search {
            &mut self.state.search_history
        } else {
            return;
        };

        history.start_navigation(current_line);
        if let Some(entry) = history.prev_match() {
            let entry = entry.to_string();
            self.state.command_line = entry.clone();
            // Clamp cursor to new line length
            self.state.command_line_cursor = self.state.command_line_cursor.min(entry.len());
        }
    }

    /// Navigate to next (newer) history entry
    pub(super) fn navigate_history_down(&mut self) {
        let history = if self.current_mode == Mode::Command {
            &mut self.state.command_history
        } else if self.current_mode == Mode::Search {
            &mut self.state.search_history
        } else {
            return;
        };

        if let Some(entry) = history.next_match() {
            let entry = entry.to_string();
            self.state.command_line = entry.clone();
            // Clamp cursor to new line length
            self.state.command_line_cursor = self.state.command_line_cursor.min(entry.len());
        }
    }
}
