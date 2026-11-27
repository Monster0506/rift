use super::ops::search::SearchRefresh;
use super::Editor;
use crate::document::ops::DocumentWrite;
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn do_undo(&mut self, count: Option<u64>) -> bool {
        crate::perf_span!("history_undo", crate::perf::PerfFields::default());
        self.replay_history(count, true, "Already at oldest change")
    }

    pub(super) fn do_redo(&mut self, count: Option<u64>) -> bool {
        crate::perf_span!("history_redo", crate::perf::PerfFields::default());
        self.replay_history(count, false, "Already at newest change")
    }

    fn replay_history(
        &mut self,
        count: Option<u64>,
        undo: bool,
        limit_message: &'static str,
    ) -> bool {
        let doc_id = self.active_document_id();
        let Ok(mut writer) = self.active_document().write() else {
            return false;
        };
        writer.clear_selection();
        let mut changed = false;
        for _ in 0..count.unwrap_or(1) {
            let step = if undo {
                DocumentWrite::undo
            } else {
                DocumentWrite::redo
            };
            if !step(&mut writer) {
                break;
            }
            changed = true;
        }
        if self.current_mode.is_visual() {
            self.set_mode(Mode::Normal);
        }
        if changed {
            self.finish_history_syntax(doc_id);
        } else {
            self.notify(crate::notification::NotificationType::Info, limit_message);
        }
        self.state.clear_command_line();
        self.schedule_search_refresh(SearchRefresh::Debounced);
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

                self.finish_history_syntax(self.active_document_id());
            }
            Err(error) => self.notify_error(error),
        }
        self.state.clear_command_line();
        self.schedule_search_refresh(SearchRefresh::Debounced);
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
