use super::Editor;
use crate::action::{Motion, OperatorType};
use crate::clipboard::Clip;
use crate::command::Command;
use crate::document::ops::EditSession;
use crate::document::ops::{CursorCause, DocumentWrite, Edit, EditSource};
use crate::document::Document;
use crate::dot_repeat::DotRegister;
use crate::executor::{execute_write_command, range_to_offsets};
use crate::mode::Mode;
use crate::term::TerminalBackend;

const fn with_count_override(cmd: Command, count: usize) -> Option<Command> {
    match cmd {
        Command::MoveBy(m, _) => Some(Command::MoveBy(m, count)),
        Command::DeleteMotion(m, _) => Some(Command::DeleteMotion(m, count)),
        Command::Change(m, _) => Some(Command::Change(m, count)),
        Command::DeleteRows(_) => Some(Command::DeleteRows(count)),
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

enum OperatorClip {
    Ring(Clip),
    Panel(super::ClipboardPanelRegister),
}

impl OperatorClip {
    fn capture(doc: &Document, clip: Clip) -> Option<Self> {
        if doc.is_clipboard_index() {
            let rows = (clip.kind() == crate::wrap::RangeKind::Linewise)
                .then(|| doc.clipboard_row_clips(&clip))
                .flatten();
            Some(Self::Panel(super::ClipboardPanelRegister { clip, rows }))
        } else if !doc.is_any_clipboard() && !clip.text().is_empty() {
            Some(Self::Ring(clip))
        } else {
            None
        }
    }
}

fn apply_change(
    doc: &mut DocumentWrite<'_>,
    command: Command,
    motion: Option<Motion>,
    count: usize,
    range: Option<crate::wrap::MotionRange>,
) -> Option<Command> {
    let record = if let Some(Motion::TextObject(spec)) = motion.filter(|_| range.is_none()) {
        let pos = crate::text_objects::resolve_insert_cursor(spec, doc.buffer(), count)?;
        doc.start_edit_session(EditSession::Change);
        let _ = doc.set_cursor(pos, CursorCause::Programmatic);
        Command::Change(Motion::TextObject(spec), 1)
    } else {
        doc.start_edit_session(EditSession::Change);
        if let Some(range) = range {
            let (start, end) = range_to_offsets(&range, doc, false);
            doc.clear_selection();
            let _ = doc.edit(Edit::delete(start..end), EditSource::User);
        } else if motion.is_none() {
            let _ = execute_write_command(command, doc, false, 1, 0, None);
        }
        command
    };
    Some(record)
}

fn apply_cut(
    doc: &mut DocumentWrite<'_>,
    command: Command,
    range: Option<crate::wrap::MotionRange>,
    ghost_clip: Option<Clip>,
    count: usize,
) -> (bool, bool) {
    let revision = doc.buffer().revision();
    let ghosted = if let Some(clip) = ghost_clip {
        let (start, end) = match &range {
            Some(range) => range_to_offsets(range, doc, true),
            None => linewise_ghost_span(doc, count),
        };
        let ghosted = start < end && doc.create_ghosts(&[(start, end)], clip);
        if ghosted {
            let _ = doc.set_cursor(start, CursorCause::Programmatic);
        }
        ghosted
    } else {
        if let Some(range) = range {
            let (start, end) = range_to_offsets(&range, doc, true);
            doc.clear_selection();
            if end > start {
                doc.edit_group("Delete", |doc| {
                    let _ = doc.edit(Edit::delete(start..end), EditSource::User);
                });
            }
        } else if matches!(command, Command::DeleteRows(_)) {
            let _ = execute_write_command(command, doc, false, 1, 0, None);
        }
        false
    };
    let changed = doc.buffer().revision() != revision;
    (changed, ghosted)
}

fn linewise_ghost_span(doc: &mut DocumentWrite<'_>, count: usize) -> (usize, usize) {
    doc.move_cursor(Motion::StartOfLine);
    let start = doc.buffer().cursor();
    for _ in 0..count.max(1) {
        if !doc.move_cursor(Motion::Down) {
            if doc.buffer().get_line() <= doc.buffer().last_real_line() {
                doc.move_cursor(Motion::EndOfLine);
                return (start.saturating_sub(1), doc.buffer().cursor());
            }
            break;
        }
    }
    (start, doc.buffer().cursor())
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn execute_operator(
        &mut self,
        op: super::pending_grammar::PendingOperator,
        motion: Motion,
    ) -> bool {
        let op = match op {
            super::pending_grammar::PendingOperator::SurroundAdd(delim_count) => {
                self.begin_surround_add(motion, delim_count);
                return true;
            }
            super::pending_grammar::PendingOperator::Op(op) => op,
        };
        let count = self
            .input
            .operator_count
            .max(1)
            .saturating_mul(self.input.count.max(1));
        self.input.operator = None;
        self.input.operator_count = 0;
        self.input.count = 0;
        self.execute_resolved_operator(op, Some(motion), count, true)
    }

    pub(super) fn execute_resolved_operator(
        &mut self,
        op: OperatorType,
        motion: Option<Motion>,
        count: usize,
        allow_ghost: bool,
    ) -> bool {
        let viewport_height = self.focused_viewport().visible_rows();
        let ghost = allow_ghost && self.state.settings.editing().ghost_cut;
        let doc_id = self.active_document_id();
        let query = self.state.last_search_query.as_deref();
        let Some(doc) = self.document_manager.get_document_mut(doc_id) else {
            return false;
        };
        if op == OperatorType::Yank {
            let clip = match motion {
                Some(motion) => crate::document::ops::compute_motion_range(
                    motion,
                    count,
                    doc,
                    viewport_height,
                    query,
                )
                .map(|range| crate::clipboard::capture_text(doc.buffer(), &range)),
                None => Some(crate::clipboard::capture_current_line(doc.buffer(), count)),
            };
            let capture = clip.and_then(|clip| OperatorClip::capture(doc, clip));
            self.publish_operator_clip(capture);
            self.set_mode(Mode::Normal);
            return true;
        }
        let mut doc = match doc.write() {
            Ok(doc) => doc,
            Err(error) => {
                if op == OperatorType::Change {
                    self.notify_error(error.into());
                }
                self.set_mode(Mode::Normal);
                return false;
            }
        };
        let range = motion.and_then(|motion| {
            if op == OperatorType::Change {
                doc.change_range(motion, count, viewport_height, query)
            } else {
                doc.motion_range(motion, count, viewport_height, query)
            }
        });
        let clip = if let Some(range) = &range {
            Some(crate::clipboard::capture_text(doc.buffer(), range))
        } else if motion.is_none() {
            Some(crate::clipboard::capture_current_line(doc.buffer(), count))
        } else {
            None
        };
        let ghost_clip = (op == OperatorType::Delete && ghost && doc.ghost_cut_allowed())
            .then(|| clip.clone())
            .flatten();
        let capture = clip.and_then(|clip| OperatorClip::capture(&doc, clip));
        let command = match (op, motion) {
            (OperatorType::Change, Some(motion)) => Command::Change(motion, count),
            (OperatorType::Change, None) => Command::ChangeLine(count),
            (_, Some(motion)) => Command::DeleteMotion(motion, count),
            (_, None) => Command::DeleteRows(count),
        };
        if op == OperatorType::Change {
            let Some(record) = apply_change(&mut doc, command, motion, count, range) else {
                self.set_mode(Mode::Normal);
                return false;
            };
            self.publish_operator_clip(capture);
            if !self.dot_repeat.is_replaying() {
                self.dot_repeat.start_insert_recording(record);
            }
            self.set_mode(Mode::Insert);
            return true;
        }

        let (changed, ghosted) = apply_cut(&mut doc, command, range, ghost_clip, count);
        if ghosted {
            self.set_recent_ghost(Some(doc_id));
        }
        self.set_mode(Mode::Normal);
        if changed {
            self.publish_operator_clip(capture);
            if !self.dot_repeat.is_replaying() {
                self.dot_repeat.record_single(command);
            }
        }
        changed
    }

    fn publish_operator_clip(&mut self, capture: Option<OperatorClip>) {
        match capture {
            Some(OperatorClip::Ring(clip)) => {
                self.yank(clip);
                self.refresh_clipboard_buffer_if_open();
            }
            Some(OperatorClip::Panel(register)) => self.clipboard_panel_register = Some(register),
            None => {}
        }
    }

    fn begin_surround_add(&mut self, motion: Motion, delim_count: usize) {
        let count = self
            .input
            .operator_count
            .max(1)
            .saturating_mul(self.input.count.max(1));
        self.input.count = 0;
        self.input.grammar = Some(super::pending_grammar::PendingGrammar::AddSurroundChar {
            motion,
            count,
            delim_count,
        });
        self.set_mode(Mode::Normal);
    }

    pub(super) fn execute_operator_linewise(&mut self, op: OperatorType) -> bool {
        self.input.operator = None;
        let count = self
            .input
            .operator_count
            .max(1)
            .saturating_mul(self.input.count.max(1));
        self.input.operator_count = 0;
        self.input.count = 0;
        self.execute_resolved_operator(op, None, count, true)
    }

    pub(super) fn execute_dot_repeat(&mut self) -> bool {
        let register = match self.dot_repeat.register() {
            Some(reg) => reg.clone(),
            None => return false,
        };

        let count = if self.input.count > 0 {
            self.input.count
        } else {
            1
        };

        self.dot_repeat.set_replaying(true);

        match register {
            DotRegister::Single(cmd) => {
                match (self.input.count > 0, with_count_override(cmd, count)) {
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
                        if let Some(doc) = self
                            .document_manager
                            .get_document_mut(self.active_document_id())
                        {
                            doc.finish_edit_session();
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
