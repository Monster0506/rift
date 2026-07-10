use super::text_object_input::{PendingTextObject, TextObjectStep};
use super::Editor;
use crate::action::{Action, EditorAction, Motion};
use crate::command::Command;
use crate::key::Key;
use crate::mode::Mode;
use crate::term::TerminalBackend;

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
    pub(super) fn advance_pending_grammar(&mut self, grammar: PendingGrammar, key: Key) {
        match grammar {
            PendingGrammar::ReplaceChar => {
                if let Key::Char(ch) = key {
                    if !self.try_run_set_aware_replace_char(ch) {
                        let count = if self.pending_count > 0 {
                            self.pending_count
                        } else {
                            1
                        };
                        let command = Command::ReplaceChar(ch, count);
                        let result = self.execute_buffer_command(command);
                        if result && !self.dot_repeat.is_replaying() {
                            self.dot_repeat.record_single(command);
                        }
                    } else {
                        self.finish_region_build(Some(Action::Editor(
                            EditorAction::ReplaceCharPending,
                        )));
                    }
                }
                self.pending_count = 0;
            }
            PendingGrammar::FindChar { forward, till } => {
                if let Key::Char(ch) = key {
                    let motion = match (forward, till) {
                        (true, false) => Motion::FindCharForward(ch),
                        (true, true) => Motion::TillCharForward(ch),
                        (false, false) => Motion::FindCharBackward(ch),
                        (false, true) => Motion::TillCharBackward(ch),
                    };
                    self.handle_action(&Action::Editor(EditorAction::Move(motion)));
                }
                self.pending_count = 0;
            }
            PendingGrammar::TextObject(mut pending) => match pending.advance(key) {
                TextObjectStep::Continue => {
                    self.pending_grammar = Some(PendingGrammar::TextObject(pending));
                }
                TextObjectStep::Finalize(spec) => self.dispatch_text_object_spec(spec),
                TextObjectStep::Cancel => {
                    self.set_mode(Mode::Normal);
                    self.pending_count = 0;
                }
            },
            PendingGrammar::SurroundVerb { count } => {
                match key {
                    Key::Char('d') => {
                        let count = if count > 0 { count } else { 1 };
                        self.pending_grammar = Some(PendingGrammar::DeleteSurround { count });
                    }
                    Key::Char('c') => {
                        let count = if count > 0 { count } else { 1 };
                        self.pending_grammar = Some(PendingGrammar::ChangeSurroundFrom { count });
                    }
                    Key::Char('g') => {
                        let delim_count = if count > 0 { count } else { 1 };
                        self.pending_operator = Some(crate::action::OperatorType::Yank);
                        self.pending_surround_add = Some(delim_count);
                        self.set_mode(Mode::OperatorPending);
                    }
                    _ => {
                        self.set_mode(Mode::Normal);
                    }
                }
                self.pending_count = 0;
            }
            PendingGrammar::DeleteSurround { count } => {
                self.set_mode(Mode::Normal);
                if let Key::Char(ch) = key {
                    if !self.try_run_set_aware_delete_surround(ch, count) {
                        let command = Command::DeleteSurround(ch, count);
                        let result = self.execute_buffer_command(command);
                        if result && !self.dot_repeat.is_replaying() {
                            self.dot_repeat.record_single(command);
                        }
                    } else {
                        self.finish_region_build(None);
                    }
                }
                self.pending_count = 0;
            }
            PendingGrammar::ChangeSurroundFrom { count } => {
                if let Key::Char(from) = key {
                    self.pending_grammar = Some(PendingGrammar::ChangeSurroundTo { from, count });
                } else {
                    self.set_mode(Mode::Normal);
                    self.pending_count = 0;
                }
            }
            PendingGrammar::ChangeSurroundTo { from, count } => {
                self.set_mode(Mode::Normal);
                if let Key::Char(to) = key {
                    if !self.try_run_set_aware_change_surround(from, to, count) {
                        let command = Command::ChangeSurround(from, to, count);
                        let result = self.execute_buffer_command(command);
                        if result && !self.dot_repeat.is_replaying() {
                            self.dot_repeat.record_single(command);
                        }
                    } else {
                        self.finish_region_build(None);
                    }
                }
                self.pending_count = 0;
            }
            PendingGrammar::AddSurroundChar {
                motion,
                count,
                delim_count,
            } => {
                self.set_mode(Mode::Normal);
                if let Key::Char(ch) = key {
                    if !self.try_run_set_aware_add_surround(ch, delim_count) {
                        let command = Command::AddSurround(motion, count, ch, delim_count);
                        let result = self.execute_buffer_command(command);
                        if result && !self.dot_repeat.is_replaying() {
                            self.dot_repeat.record_single(command);
                        }
                    } else {
                        self.finish_region_build(Some(Action::Editor(
                            EditorAction::AddSurroundToSet { ch, delim_count },
                        )));
                    }
                }
                self.pending_count = 0;
            }
        }
    }
}
