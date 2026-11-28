use super::Editor;
use crate::action::{Motion, OperatorType};
use crate::clipboard::Clip;
use crate::command::Command;
use crate::document::ops::EditSession;
use crate::document::ops::{CursorCause, DeleteLanding, DocumentWrite, Edit, EditSource};
use crate::document::{DirectoryTransfer, Document, TransferKind};
use crate::dot_repeat::DotRegister;
use crate::executor::{execute_write_command, range_to_offsets};
use crate::mode::Mode;
use crate::selection::TargetSet;
use crate::term::TerminalBackend;
use crate::wrap::RangeKind;

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
    Ring { first: Clip, rest: Vec<Clip> },
    Panel(super::ClipboardPanelRegister),
}

impl OperatorClip {
    fn capture(doc: &Document, targets: &TargetSet) -> Option<Self> {
        let (first, rest) = targets.targets().split_last()?;
        if doc.is_clipboard_index() {
            let clip = crate::clipboard::capture_target(doc.buffer(), first);
            let rows = doc.clipboard_target_rows(targets);
            Some(Self::Panel(super::ClipboardPanelRegister { clip, rows }))
        } else if doc.is_any_clipboard() {
            None
        } else {
            let mut clips = rest
                .iter()
                .chain(std::iter::once(first))
                .map(|target| crate::clipboard::capture_target(doc.buffer(), target))
                .filter(|clip| !clip.text().is_empty())
                .rev();
            let first = clips.next()?;
            let rest = clips.collect();
            Some(Self::Ring { first, rest })
        }
    }

    const fn most_recent(&self) -> &Clip {
        match self {
            Self::Ring { first, .. } => first,
            Self::Panel(register) => &register.clip,
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
            let (start, end) = range_to_offsets(&range, doc);
            doc.clear_selection();
            let _ = doc.edit(Edit::delete(start..end), EditSource::User);
        } else if motion.is_none() {
            let _ = execute_write_command(command, doc, false, 1, 0, None);
        }
        command
    };
    Some(record)
}

#[derive(Clone, Copy)]
enum Transfer {
    Cut { landing: DeleteLanding, ghost: bool },
    Yank { cursor: Option<usize> },
}

struct TransferResult {
    capture: Option<OperatorClip>,
    entries: Option<Vec<DirectoryTransfer>>,
    changed: bool,
    ghosted: bool,
}

impl TransferResult {
    fn capture(doc: &Document, targets: &TargetSet, kind: TransferKind) -> Self {
        Self {
            capture: OperatorClip::capture(doc, targets),
            entries: doc.capture_directory_targets(targets, kind),
            changed: false,
            ghosted: false,
        }
    }
}

fn transfer_targets(
    doc: &mut Document,
    targets: &TargetSet,
    transfer: Transfer,
) -> Option<TransferResult> {
    match transfer {
        Transfer::Cut { landing, ghost } => {
            let mut doc = doc.write().ok()?;
            let mut result = TransferResult::capture(&doc, targets, TransferKind::Move);
            let clip = result
                .capture
                .as_ref()
                .filter(|_| ghost)
                .map(OperatorClip::most_recent);
            doc.clear_target_selection();
            (result.changed, result.ghosted) = doc.delete_targets(targets, landing, clip);
            if !result.changed {
                result.capture = None;
                result.entries = None;
            }
            Some(result)
        }
        Transfer::Yank { cursor } => {
            let result = TransferResult::capture(doc, targets, TransferKind::Copy);
            doc.clear_target_selection();
            if let Some(start) = cursor {
                let _ = doc.set_cursor(start, CursorCause::Programmatic);
            }
            Some(result)
        }
    }
}

fn resolve_operator_targets(
    doc: &mut Document,
    motion: Option<Motion>,
    count: usize,
    viewport_height: usize,
    query: Option<&str>,
) -> Option<TargetSet> {
    match motion {
        Some(motion) => doc.motion_targets(motion, count, viewport_height, query),
        None => TargetSet::current_lines(doc.buffer(), count),
    }
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
        if op == OperatorType::Change {
            let mut doc = match doc.write() {
                Ok(doc) => doc,
                Err(error) => {
                    self.notify_error(error.into());
                    self.set_mode(Mode::Normal);
                    return false;
                }
            };
            let range =
                motion.and_then(|motion| doc.change_range(motion, count, viewport_height, query));
            let targets = if let Some(range) = &range {
                TargetSet::from_motion(range, doc.buffer())
            } else if motion.is_none() {
                TargetSet::current_lines(doc.buffer(), count)
            } else {
                None
            };
            let capture = targets
                .as_ref()
                .and_then(|targets| OperatorClip::capture(&doc, targets));
            let command = motion.map_or(Command::ChangeLine(count), |motion| {
                Command::Change(motion, count)
            });
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
        let transfer = match op {
            OperatorType::Yank => Transfer::Yank { cursor: None },
            _ => Transfer::Cut {
                landing: if motion.is_none() {
                    DeleteLanding::CountedRows
                } else {
                    DeleteLanding::AtEdit
                },
                ghost,
            },
        };
        let outcome = resolve_operator_targets(doc, motion, count, viewport_height, query)
            .and_then(|targets| transfer_targets(doc, &targets, transfer));
        self.set_mode(Mode::Normal);
        if let Some(outcome) = outcome {
            let changed = outcome.changed;
            self.publish_transfer(outcome);
            if changed && !self.dot_repeat.is_replaying() {
                let command = motion.map_or(Command::DeleteRows(count), |motion| {
                    Command::DeleteMotion(motion, count)
                });
                self.dot_repeat.record_single(command);
            }
            return changed || op == OperatorType::Yank;
        }
        op == OperatorType::Yank
    }

    pub(super) fn execute_selection_operator(&mut self, op: OperatorType) -> bool {
        let visual = self.current_mode.visual_range_kind();
        let ghost = self.state.settings.editing().ghost_cut;
        let doc = self.active_document();
        let Some(targets) = doc.selection_targets(visual) else {
            return false;
        };
        let transfer = match op {
            OperatorType::Delete => Transfer::Cut {
                landing: if visual == Some(RangeKind::Linewise) {
                    DeleteLanding::FirstNonblank
                } else {
                    DeleteLanding::AtEdit
                },
                ghost,
            },
            OperatorType::Yank => Transfer::Yank {
                cursor: visual
                    .and_then(|kind| doc.visual_region(kind))
                    .map(|region| region.resolve(doc.buffer()).start),
            },
            OperatorType::Change => return false,
        };
        if let Some(outcome) = transfer_targets(doc, &targets, transfer) {
            self.publish_transfer(outcome);
            self.set_mode(Mode::Normal);
        }
        true
    }

    pub(super) fn put_over_selection(&mut self, clip: &Clip, kind: RangeKind, swap: bool) -> bool {
        let count = self.input.count.max(1);
        let doc = self.active_document();
        let Some(targets) = doc.selection_targets(Some(kind)) else {
            return false;
        };
        let Ok(mut doc) = doc.write() else {
            return false;
        };
        let removed = swap
            .then(|| OperatorClip::capture(&doc, &targets))
            .flatten();
        if !doc.put_over_targets(clip, &targets, count) {
            return false;
        }
        doc.clear_target_selection();
        self.commit_all_pending_ghosts();
        self.publish_operator_clip(removed);
        self.set_mode(Mode::Normal);
        self.finish_region_build(None);
        true
    }

    fn publish_transfer(&mut self, outcome: TransferResult) {
        if outcome.ghosted {
            self.set_recent_ghost(Some(self.active_document_id()));
        }
        self.publish_operator_clip(outcome.capture);
        self.set_explorer_register(outcome.entries);
    }

    fn publish_operator_clip(&mut self, capture: Option<OperatorClip>) {
        match capture {
            Some(OperatorClip::Ring { first, rest }) => {
                for clip in rest.into_iter().rev() {
                    self.yank(clip);
                }
                self.yank(first);
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
