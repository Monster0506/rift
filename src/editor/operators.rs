use super::Editor;
use crate::action::Motion;
use crate::command::Command;
use crate::dot_repeat::DotRegister;
use crate::mode::Mode;
use crate::term::TerminalBackend;

const fn with_count_override(cmd: Command, count: usize) -> Option<Command> {
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
    pub(super) fn execute_operator(&mut self, op: super::PendingOperator, motion: Motion) -> bool {
        let op = match op {
            super::PendingOperator::SurroundAdd(delim_count) => {
                self.begin_surround_add(motion, delim_count);
                return true;
            }
            super::PendingOperator::Op(op) => op,
        };
        let count = self
            .pending_operator_count
            .max(1)
            .saturating_mul(self.pending_count.max(1));
        self.pending_operator = None;
        self.pending_operator_count = 0;
        self.pending_count = 0;
        if op != crate::action::OperatorType::Yank
            && self.active_doc_is(crate::document::Document::is_read_only)
        {
            self.set_mode(Mode::Normal);
            return false;
        }

        let viewport_height = self.render_system.viewport.visible_rows();
        let last_search_query = self.state.last_search_query.clone();
        let is_change = op == crate::action::OperatorType::Change;
        let motion_range = self.document_manager.active_document_mut().and_then(|doc| {
            let compute = if is_change {
                crate::executor::compute_change_range
            } else {
                crate::executor::compute_motion_range
            };
            compute(
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
        let cut = self.push_clip(captured, op);

        match op {
            crate::action::OperatorType::Delete => {
                self.execute_delete_operator(motion, count, motion_range.zip(cut))
            }
            crate::action::OperatorType::Change => {
                if self.active_doc_is(super::super::document::Document::is_read_only) {
                    self.reject_read_only_edit();
                    self.set_mode(Mode::Normal);
                    return false;
                }
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
                self.set_mode(Mode::Normal);
                true
            }
        }
    }

    fn push_clip(
        &mut self,
        clip: Option<crate::clipboard::Clip>,
        op: crate::action::OperatorType,
    ) -> Option<crate::clipboard::Clip> {
        let clip = clip?;
        let cut = (op == crate::action::OperatorType::Delete).then(|| clip.clone());
        let in_clipboard = self.active_doc_is(super::super::document::Document::is_any_clipboard);
        if self.active_doc_is(super::super::document::Document::is_clipboard_index) {
            let rows = if clip.kind() == crate::wrap::RangeKind::Linewise {
                self.document_manager
                    .active_document()
                    .and_then(|doc| doc.clipboard_row_clips(&clip))
            } else {
                None
            };
            self.clipboard_panel_register = Some(super::ClipboardPanelRegister {
                clip: clip.clone(),
                rows,
            });
        } else if !in_clipboard && !clip.text().is_empty() {
            self.clipboard_ring.push(clip);
            self.refresh_clipboard_buffer_if_open();
        }
        cut
    }

    fn begin_surround_add(&mut self, motion: Motion, delim_count: usize) {
        let count = self
            .pending_operator_count
            .max(1)
            .saturating_mul(self.pending_count.max(1));
        self.pending_count = 0;
        self.pending_grammar = Some(super::pending_grammar::PendingGrammar::AddSurroundChar {
            motion,
            count,
            delim_count,
        });
        self.set_mode(Mode::Normal);
    }

    fn execute_delete_operator(
        &mut self,
        motion: Motion,
        count: usize,
        cut: Option<(crate::wrap::MotionRange, crate::clipboard::Clip)>,
    ) -> bool {
        let command = crate::command::Command::Delete(motion, count);
        self.set_mode(Mode::Normal);
        let ghost_allowed = self
            .document_manager
            .active_document()
            .is_some_and(super::super::document::Document::ghost_cut_allowed);
        let result = if self.state.settings.editing.ghost_cut && ghost_allowed {
            let mut ghosted_doc = None;
            if let Some((range, clip)) = cut {
                if let Some(doc) = self.document_manager.active_document_mut() {
                    let (start, end) = crate::executor::range_to_offsets(&range, doc, true);
                    if end > start {
                        doc.create_ghosts(&[(start, end)], clip);
                        let _ = doc.buffer.set_cursor(start);
                        ghosted_doc = Some(doc.id);
                    }
                }
            }
            if ghosted_doc.is_some() {
                self.do_incremental_syntax_parse();
            }
            if let Some(id) = ghosted_doc {
                self.document_manager.set_most_recent_ghost_doc(Some(id));
            }
            ghosted_doc.is_some()
        } else {
            self.execute_buffer_command(command)
        };
        if result && !self.dot_repeat.is_replaying() && command.is_repeatable() {
            self.dot_repeat.record_single(command);
        }
        result
    }

    pub(super) fn execute_operator_linewise(&mut self, op: crate::action::OperatorType) -> bool {
        self.pending_operator = None;
        let count = self
            .pending_operator_count
            .max(1)
            .saturating_mul(self.pending_count.max(1));
        self.pending_operator_count = 0;
        self.pending_count = 0;
        if op != crate::action::OperatorType::Yank
            && self.active_doc_is(crate::document::Document::is_read_only)
        {
            self.set_mode(Mode::Normal);
            return false;
        }

        let captured = self
            .document_manager
            .active_document()
            .map(|doc| crate::clipboard::capture_current_line(&doc.buffer, count));
        let cut = self.push_clip(captured, op);

        match op {
            crate::action::OperatorType::Delete => {
                let command = crate::command::Command::DeleteLine(count);
                self.set_mode(Mode::Normal);
                let ghost_allowed = self
                    .document_manager
                    .active_document()
                    .is_some_and(super::super::document::Document::ghost_cut_allowed);
                let result = if self.state.settings.editing.ghost_cut && ghost_allowed {
                    let mut ghosted_doc = None;
                    if let Some((doc, clip)) = self.document_manager.active_document_mut().zip(cut)
                    {
                        doc.buffer.move_to_line_start();
                        let start = doc.buffer.cursor();
                        let mut reached_last_line = false;
                        for _ in 0..count.max(1) {
                            if !doc.buffer.move_down() {
                                reached_last_line =
                                    doc.buffer.get_line() <= doc.buffer.last_real_line();
                                break;
                            }
                        }
                        let (ghost_start, ghost_end) = if reached_last_line {
                            doc.buffer.move_to_line_end();
                            let end = doc.buffer.cursor();
                            if start > 0 {
                                (start - 1, end)
                            } else {
                                (start, end)
                            }
                        } else {
                            (start, doc.buffer.cursor())
                        };
                        let _ = doc.buffer.set_cursor(ghost_start);
                        if ghost_end > ghost_start {
                            doc.create_ghosts(&[(ghost_start, ghost_end)], clip);
                            ghosted_doc = Some(doc.id);
                        }
                    }
                    if ghosted_doc.is_some() {
                        self.do_incremental_syntax_parse();
                    }
                    if let Some(id) = ghosted_doc {
                        self.document_manager.set_most_recent_ghost_doc(Some(id));
                    }
                    ghosted_doc.is_some()
                } else {
                    self.execute_buffer_command(command)
                };
                if result && !self.dot_repeat.is_replaying() && command.is_repeatable() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            crate::action::OperatorType::Change => {
                if self.active_doc_is(super::super::document::Document::is_read_only) {
                    self.reject_read_only_edit();
                    self.set_mode(Mode::Normal);
                    return false;
                }
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
                self.set_mode(Mode::Normal);
                true
            }
        }
    }

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
            DotRegister::InsertSession {
                entry,
                commands,
                region_actions,
            } => {
                for _ in 0..count {
                    if region_actions.is_empty() {
                        self.handle_mode_management(entry);
                    } else {
                        for action in &region_actions {
                            self.handle_action(action);
                        }
                        if self.current_mode != Mode::Insert {
                            continue;
                        }
                    }

                    for &cmd in &commands {
                        self.execute_buffer_command(cmd);
                    }

                    if region_actions.is_empty() {
                        if let Some(doc) = self.document_manager.active_document_mut() {
                            doc.commit_transaction();
                        }
                        self.set_mode(Mode::Normal);
                    } else {
                        self.handle_action(&crate::action::Action::Editor(
                            crate::action::EditorAction::EnterNormalMode,
                        ));
                    }
                }
            }
            DotRegister::RegionBuildSession { actions, follow_up } => {
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
