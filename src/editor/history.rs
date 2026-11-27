use super::Editor;
use crate::error::{ErrorType, RiftError};
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn do_undo(&mut self, count: Option<u64>) -> bool {
        crate::perf_span!("history_undo", crate::perf::PerfFields::default());
        self.replay_history(
            count,
            crate::document::Document::undo,
            "Already at oldest change",
        )
    }

    pub(super) fn do_redo(&mut self, count: Option<u64>) -> bool {
        crate::perf_span!("history_redo", crate::perf::PerfFields::default());
        self.replay_history(
            count,
            crate::document::Document::redo,
            "Already at newest change",
        )
    }

    fn replay_history(
        &mut self,
        count: Option<u64>,
        step: fn(&mut crate::document::Document) -> bool,
        limit_message: &'static str,
    ) -> bool {
        if self.active_document().check_edit_access().is_err() {
            return false;
        }
        if self.current_mode.is_visual() {
            self.set_mode(Mode::Normal);
        }
        let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        else {
            return false;
        };
        let doc_id = doc.id;
        doc.clear_selection();
        let mut changed = false;
        for _ in 0..count.unwrap_or(1) {
            if !step(doc) {
                break;
            }
            changed = true;
        }
        if changed {
            self.drain_syntax_sync();
        } else {
            self.notify(crate::notification::NotificationType::Info, limit_message);
        }
        self.state.clear_command_line();
        self.schedule_search_refresh();
        if changed {
            self.spawn_syntax_parse_job_immediate(doc_id);
        }
        true
    }

    pub(super) fn create_checkpoint(&mut self) -> bool {
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            doc.checkpoint();
            self.notify(
                crate::notification::NotificationType::Info,
                "Checkpoint created",
            );
        }
        true
    }

    pub(super) fn do_undo_goto(&mut self, seq: u64) {
        let doc = self
            .document_manager
            .get_document_mut(self.active_document_id())
            .unwrap();
        match doc.goto_seq(seq) {
            Ok(()) => {
                self.notify(
                    crate::notification::NotificationType::Info,
                    format!("Jumped to edit #{seq}"),
                );

                self.spawn_syntax_parse_job_immediate(self.active_document_id());
            }
            Err(e) => {
                self.notify_error(RiftError::new(
                    ErrorType::Execution,
                    "UNDO_ERROR",
                    e.to_string(),
                ));
            }
        }
        self.state.clear_command_line();
        self.schedule_search_refresh();
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
            self.state.command_line.clone_from(&entry);
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
            self.state.command_line.clone_from(&entry);
            // Clamp cursor to new line length
            self.state.command_line_cursor = self.state.command_line_cursor.min(entry.len());
        }
    }
}
