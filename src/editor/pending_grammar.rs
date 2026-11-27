use super::text_object_input::{PendingTextObject, TextObjectStep};
use super::Editor;
use crate::action::{Action, EditorAction, Motion};
use crate::command::Command;
use crate::key::Key;
use crate::mode::Mode;
use crate::term::TerminalBackend;

#[derive(Default)]
pub(super) struct InputState {
    pub(super) keys: Vec<Key>,
    pub(super) count: usize,
    pub(super) operator_count: usize,
    pub(super) operator: Option<PendingOperator>,
    pub(super) grammar: Option<PendingGrammar>,
    pub(super) multi_insert_anchors: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingOperator {
    Op(crate::action::OperatorType),
    SurroundAdd(usize),
}

/// A multi-key input grammar in progress: the next keypress is consumed by
/// the grammar itself rather than going through the normal keymap trie.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PendingGrammar {
    ReplaceChar,
    FindChar {
        forward: bool,
        till: bool,
    },
    TextObject(PendingTextObject),
    /// `s<verb>`: next key selects the surround command -- `d` (delete), `c` (change),
    /// or `g` (add). `count` is the delimiter repeat count typed before `s`.
    SurroundVerb {
        count: usize,
    },
    /// `sd<ch>`: next key is the surround char to delete. `count` repeats the
    /// delimiter on each side.
    DeleteSurround {
        count: usize,
    },
    /// `sc<from>`: next key is the existing surround char to match. `count`
    /// carries through to both matching and the eventual replacement.
    ChangeSurroundFrom {
        count: usize,
    },
    /// `sc<from><to>`: next key is the replacement surround char.
    ChangeSurroundTo {
        from: char,
        count: usize,
    },
    /// `sg<motion><ch>`: next key is the delimiter to wrap the resolved range
    /// in. `delim_count` repeats that delimiter on each side.
    AddSurroundChar {
        motion: Motion,
        count: usize,
        delim_count: usize,
    },
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn read_replace_char(&mut self) -> bool {
        self.input.grammar = Some(PendingGrammar::ReplaceChar);
        true
    }

    pub(super) fn advance_pending_grammar(&mut self, grammar: PendingGrammar, key: Key) {
        match grammar {
            PendingGrammar::ReplaceChar => self.handle_replace_char_grammar(key),
            PendingGrammar::FindChar { forward, till } => {
                if let Some(ch) = key.text() {
                    let motion = match (forward, till) {
                        (true, false) => Motion::FindCharForward(ch),
                        (true, true) => Motion::TillCharForward(ch),
                        (false, false) => Motion::FindCharBackward(ch),
                        (false, true) => Motion::TillCharBackward(ch),
                    };
                    self.handle_action(&Action::Editor(EditorAction::Move(motion)));
                }
                self.input.count = 0;
            }
            PendingGrammar::TextObject(mut pending) => match pending.advance(key) {
                TextObjectStep::Continue => {
                    self.input.grammar = Some(PendingGrammar::TextObject(pending));
                }
                TextObjectStep::Finalize(spec) => self.dispatch_text_object_spec(spec),
                TextObjectStep::Cancel => {
                    self.set_mode(Mode::Normal);
                    self.input.count = 0;
                }
            },
            PendingGrammar::SurroundVerb { count } => self.advance_surround_verb(count, key),
            PendingGrammar::DeleteSurround { count } => {
                self.set_mode(Mode::Normal);
                if let Some(ch) = key.text() {
                    match self.try_run_set_aware_delete_surround(ch, count) {
                        Ok(true) => self.finish_region_build(None),
                        Ok(false) => {
                            self.execute_and_record(Command::DeleteSurround(ch, count));
                        }
                        Err(_) => {}
                    }
                }
                self.input.count = 0;
            }
            PendingGrammar::ChangeSurroundFrom { count } => {
                if let Some(from) = key.text() {
                    self.input.grammar = Some(PendingGrammar::ChangeSurroundTo { from, count });
                } else {
                    self.set_mode(Mode::Normal);
                    self.input.count = 0;
                }
            }
            PendingGrammar::ChangeSurroundTo { from, count } => {
                self.set_mode(Mode::Normal);
                if let Some(to) = key.text() {
                    match self.try_run_set_aware_change_surround(from, to, count) {
                        Ok(true) => self.finish_region_build(None),
                        Ok(false) => {
                            self.execute_and_record(Command::ChangeSurround(from, to, count));
                        }
                        Err(_) => {}
                    }
                }
                self.input.count = 0;
            }
            PendingGrammar::AddSurroundChar {
                motion,
                count,
                delim_count,
            } => {
                self.set_mode(Mode::Normal);
                if let Some(ch) = key.text() {
                    match self.try_run_set_aware_add_surround(ch, delim_count) {
                        Ok(true) => self.finish_region_build(Some(Action::Editor(
                            EditorAction::AddSurroundToSet { ch, delim_count },
                        ))),
                        Ok(false) => {
                            self.execute_and_record(Command::AddSurround(
                                motion,
                                count,
                                ch,
                                delim_count,
                            ));
                        }
                        Err(_) => {}
                    }
                }
                self.input.count = 0;
            }
        }
    }

    fn handle_replace_char_grammar(&mut self, key: Key) {
        if matches!(
            self.region_build_recording.last(),
            Some(Action::Editor(EditorAction::ReplaceCharPending))
        ) {
            self.region_build_recording.pop();
        }

        let ch = key.text().or_else(|| {
            if !self.current_mode.is_visual() {
                return None;
            }
            match key {
                Key::TAB => Some('\t'),
                Key::ENTER => Some('\r'),
                _ => None,
            }
        });
        if let Some(ch) = ch {
            if !self.handle_action(&Action::Editor(EditorAction::ReplaceSelection(ch))) {
                let count = if self.input.count > 0 {
                    self.input.count
                } else {
                    1
                };
                let command = Command::ReplaceChar(ch, count);
                self.execute_and_record(command);
            }
        }
        self.input.count = 0;
    }

    pub(super) fn replace_selection(&mut self, ch: char) -> bool {
        let visual_kind = self.current_mode.visual_range_kind();
        if visual_kind.is_none() && self.active_document().selections().is_empty() {
            return false;
        }
        let start = self
            .visual_anchor()
            .map(|anchor| anchor.min(self.active_document().buffer().cursor()));
        let Ok(result) = self.replace_banked_regions(ch, visual_kind, start) else {
            return true;
        };
        if visual_kind.is_some() {
            self.set_mode(Mode::Normal);
        }
        let was_visual = visual_kind.is_some();
        let follow_up = (!was_visual).then_some(Action::Editor(EditorAction::ReplaceSelection(ch)));
        self.finish_region_build(follow_up);
        result || was_visual
    }

    fn advance_surround_verb(&mut self, count: usize, key: Key) {
        match key.text() {
            Some('d') => {
                let count = if count > 0 { count } else { 1 };
                self.input.grammar = Some(PendingGrammar::DeleteSurround { count });
            }
            Some('c') => {
                let count = if count > 0 { count } else { 1 };
                self.input.grammar = Some(PendingGrammar::ChangeSurroundFrom { count });
            }
            Some('g') => {
                let delim_count = if count > 0 { count } else { 1 };
                self.input.operator = Some(PendingOperator::SurroundAdd(delim_count));
                self.set_mode(Mode::OperatorPending);
            }
            _ => {
                self.set_mode(Mode::Normal);
            }
        }
        self.input.count = 0;
    }

    fn execute_and_record(&mut self, command: Command) {
        let result = self.execute_buffer_command(command);
        if result && !self.dot_repeat.is_replaying() {
            self.dot_repeat.record_single(command);
        }
    }
}
