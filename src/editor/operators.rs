use super::Editor;
use crate::action::Motion;
use crate::command::Command;
use crate::dot_repeat::DotRegister;
use crate::mode::Mode;
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
        let motion_range = self.document_manager.active_document_mut().and_then(|doc| {
            crate::executor::compute_motion_range(
                motion,
                count,
                doc,
                viewport_height,
                last_search_query.as_deref(),
            )
        });
        let captured = motion_range.clone().and_then(|range| {
            self.document_manager
                .active_document()
                .map(|doc| crate::clipboard::capture_text(&doc.buffer, &range))
        });
        let has_range = captured.is_some();
        let in_clipboard = self.active_doc_is(|d| d.is_any_clipboard());
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
                // Commit any pending ghost, then recompute fresh: reusing
                // `motion_range` would target offsets from before that shift.
                if let Some(doc) = self.document_manager.active_document_mut() {
                    doc.commit_pending_ghost();
                }
                let fresh_range = self.document_manager.active_document_mut().and_then(|doc| {
                    crate::executor::compute_motion_range(
                        motion,
                        count,
                        doc,
                        viewport_height,
                        last_search_query.as_deref(),
                    )
                });
                let mut ghosted_doc = None;
                if let Some(range) = fresh_range {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        let (start, end) = crate::executor::range_to_offsets(&range, doc, true);
                        if end > start {
                            doc.create_ghosts(&[(start, end)]);
                            let _ = doc.buffer.set_cursor(start);
                            ghosted_doc = Some(doc.id);
                        }
                    }
                }
                let result = ghosted_doc.is_some();
                if let Some(id) = ghosted_doc {
                    self.document_manager.set_most_recent_ghost_doc(Some(id));
                }
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
        let in_clipboard = self.active_doc_is(|d| d.is_any_clipboard());
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
                let mut ghosted_doc = None;
                if let Some(doc) = self.document_manager.active_document_mut() {
                    // Commit any pending ghost first: its commit can shift the
                    // buffer, so the line range below must reflect that.
                    doc.commit_pending_ghost();
                    doc.buffer.move_to_line_start();
                    let start = doc.buffer.cursor();
                    let mut reached_last_line = false;
                    for _ in 0..count.max(1) {
                        if !doc.buffer.move_down() {
                            reached_last_line = true;
                            break;
                        }
                    }
                    let (ghost_start, ghost_end) = if !reached_last_line {
                        (start, doc.buffer.cursor())
                    } else {
                        doc.buffer.move_to_line_end();
                        let end = doc.buffer.cursor();
                        // Last line: ghost the preceding newline too, mirroring
                        // DeleteLine's extra delete_backward for that case.
                        if start > 0 { (start - 1, end) } else { (start, end) }
                    };
                    let _ = doc.buffer.set_cursor(ghost_start);
                    if ghost_end > ghost_start {
                        doc.create_ghosts(&[(ghost_start, ghost_end)]);
                        ghosted_doc = Some(doc.id);
                    }
                }
                let result = ghosted_doc.is_some();
                if let Some(id) = ghosted_doc {
                    self.document_manager.set_most_recent_ghost_doc(Some(id));
                }
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
