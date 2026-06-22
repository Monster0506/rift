use super::Editor;
use crate::action::Motion;
use crate::command::Command;
use crate::dot_repeat::DotRegister;
use crate::mode::Mode;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

/// Rebuild `cmd` with its embedded count replaced by `count`, for command
/// variants that carry one. `None` for variants with no count to override.
fn with_count_override(cmd: Command, count: usize) -> Option<Command> {
    match cmd {
        Command::Move(m, _) => Some(Command::Move(m, count)),
        Command::Delete(m, _) => Some(Command::Delete(m, count)),
        Command::Change(m, _) => Some(Command::Change(m, count)),
        Command::DeleteLine(_) => Some(Command::DeleteLine(count)),
        Command::ChangeLine(_) => Some(Command::ChangeLine(count)),
        Command::ReplaceChar(ch, _) => Some(Command::ReplaceChar(ch, count)),
        Command::DeleteSurround(ch, _) => Some(Command::DeleteSurround(ch, count)),
        Command::ChangeSurround(from, to, _) => Some(Command::ChangeSurround(from, to, count)),
        Command::AddSurround(m, _, ch, delim_count) => {
            Some(Command::AddSurround(m, count, ch, delim_count))
        }
        _ => None,
    }
}

impl<T: TerminalBackend> Editor<T> {
    pub fn term_mut(&mut self) -> &mut T {
        &mut self.term
    }

    pub(super) fn execute_operator(
        &mut self,
        op: crate::action::OperatorType,
        motion: Motion,
    ) -> bool {
        // .take() unconditionally so a stale flag from an interrupted `sg`
        // never leaks into an unrelated operator below.
        if let Some(delim_count) = self.pending_surround_add.take() {
            if op == crate::action::OperatorType::Yank {
                let count = self.pending_operator_count.max(1) * self.pending_count.max(1);
                self.pending_operator = None;
                self.pending_operator_count = 0;
                self.pending_count = 0;
                self.pending_grammar =
                    Some(super::pending_grammar::PendingGrammar::AddSurroundChar {
                        motion,
                        count,
                        delim_count,
                    });
                return true;
            }
        }
        let count = self.pending_operator_count.max(1) * self.pending_count.max(1);
        self.pending_operator = None;
        self.pending_operator_count = 0;
        self.pending_count = 0;

        // Capture text to ring before any destructive operation, and for yank.
        let viewport_height = self.render_system.viewport.visible_rows();
        let last_search_query = self.state.last_search_query.clone();
        let captured = self.document_manager.active_document_mut().and_then(|doc| {
            let tab_width = doc.options.tab_width;
            crate::executor::compute_motion_range(
                motion,
                count,
                doc,
                viewport_height,
                last_search_query.as_deref(),
                tab_width,
            )
            .map(|range| crate::clipboard::capture_text(&doc.buffer, &range))
        });
        let has_range = captured.is_some();
        let in_clipboard = self
            .document_manager
            .active_document()
            .map(|d| d.is_any_clipboard())
            .unwrap_or(false);
        if let Some(text) = captured.filter(|s| !s.is_empty()) {
            if !in_clipboard {
                self.clipboard_ring.push(text);
                self.refresh_clipboard_buffer_if_open();
            }
        }

        match op {
            crate::action::OperatorType::Delete => {
                let command = crate::command::Command::Delete(motion, count);
                self.set_mode(Mode::Normal);
                let result = self.execute_buffer_command(command);
                if result && !self.dot_repeat.is_replaying() && command.is_repeatable() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            crate::action::OperatorType::Change => {
                if !has_range {
                    if let Motion::TextObject(spec) = motion {
                        let insert_pos =
                            self.document_manager.active_document_mut().and_then(|doc| {
                                crate::text_objects::resolve_insert_cursor(spec, &doc.buffer, count)
                            });
                        if let Some(pos) = insert_pos {
                            if let Some(doc) = self.document_manager.active_document_mut() {
                                let _ = doc.buffer.set_cursor(pos);
                            }
                            if !self.dot_repeat.is_replaying() {
                                let cmd = crate::command::Command::Change(motion, 1);
                                self.dot_repeat.start_insert_recording(cmd);
                            }
                            self.set_mode(Mode::Insert);
                            return true;
                        }
                        self.set_mode(Mode::Normal);
                        return false;
                    }
                }
                let command = crate::command::Command::Change(motion, count);
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction("Change");
                self.set_mode(Mode::Normal);
                self.execute_buffer_command(command);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
                true
            }
            crate::action::OperatorType::Yank => {
                // Text already captured above; just return to Normal.
                self.set_mode(Mode::Normal);
                true
            }
        }
    }

    pub(super) fn execute_operator_linewise(&mut self, op: crate::action::OperatorType) -> bool {
        self.pending_operator = None;
        self.pending_surround_add = None;
        // Consume both counts and clear them so neither leaks into the
        // next motion.
        let count = self.pending_operator_count.max(1) * self.pending_count.max(1);
        self.pending_operator_count = 0;
        self.pending_count = 0;

        // Capture current line(s) text for all operators.
        let captured = self
            .document_manager
            .active_document()
            .map(|doc| crate::clipboard::capture_current_line(&doc.buffer, count));
        let in_clipboard = self
            .document_manager
            .active_document()
            .map(|d| d.is_any_clipboard())
            .unwrap_or(false);
        if let Some(text) = captured.filter(|s| !s.is_empty()) {
            if !in_clipboard {
                self.clipboard_ring.push(text);
                self.refresh_clipboard_buffer_if_open();
            }
        }

        match op {
            crate::action::OperatorType::Delete => {
                let command = crate::command::Command::DeleteLine(count);
                self.set_mode(Mode::Normal);
                let result = self.execute_buffer_command(command);
                if result && !self.dot_repeat.is_replaying() && command.is_repeatable() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            crate::action::OperatorType::Change => {
                let command = crate::command::Command::ChangeLine(count);
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction("Change");
                self.set_mode(Mode::Normal);
                self.execute_buffer_command(command);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
                true
            }
            crate::action::OperatorType::Yank => {
                // Text already captured above; just return to Normal.
                self.set_mode(Mode::Normal);
                true
            }
        }
    }

    /// Replay the last repeatable action (dot-repeat)
    pub(super) fn execute_dot_repeat(&mut self) -> bool {
        let register = match self.dot_repeat.register() {
            Some(reg) => reg.clone(),
            None => return false,
        };

        let count = if self.pending_count > 0 {
            self.pending_count
        } else {
            1
        };

        self.dot_repeat.set_replaying(true);

        match register {
            DotRegister::Single(cmd) => {
                // A leading count on `.` replaces (not multiplies) an
                // embedded count: 3. after d2w runs d3w once, matching vim.
                match (self.pending_count > 0, with_count_override(cmd, count)) {
                    (true, Some(overridden)) => {
                        self.execute_buffer_command(overridden);
                    }
                    _ => {
                        for _ in 0..count {
                            self.execute_buffer_command(cmd);
                        }
                    }
                }
            }
            DotRegister::InsertSession { entry, commands } => {
                for _ in 0..count {
                    // Enter insert mode (handles cursor positioning for a/A/I)
                    self.handle_mode_management(entry);

                    // Replay all commands from the session
                    for &cmd in &commands {
                        self.execute_buffer_command(cmd);
                    }

                    // Exit insert mode: commit transaction
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.commit_transaction();
                    }
                    self.set_mode(Mode::Normal);
                }
            }
            DotRegister::RegionBuildSession { actions, follow_up } => {
                // Rebuild relative to the current cursor by replaying the
                // recorded actions; count doesn't apply (would re-bank).
                for action in &actions {
                    self.handle_action(action);
                }
                if let Some(follow_up) = &follow_up {
                    self.handle_action(follow_up);
                }
            }
        }

        self.dot_repeat.set_replaying(false);
        true
    }
}
