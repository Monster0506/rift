use super::text_object_input::{PendingTextObject, TextObjectStep};
use super::Editor;
use crate::action::{Action, EditorAction, Motion};
use crate::command::Command;
use crate::key::Key;
use crate::mode::Mode;
#[allow(unused_imports)]
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
    /// `ds<ch>`: next key is the surround char to delete. `count` repeats the
    /// delimiter on each side.
    DeleteSurround {
        count: usize,
    },
    /// `cs<from>`: next key is the existing surround char to match. `count`
    /// carries through to both matching and the eventual replacement.
    ChangeSurroundFrom {
        count: usize,
    },
    /// `cs<from><to>`: next key is the replacement surround char.
    ChangeSurroundTo {
        from: char,
        count: usize,
    },
    /// `ys<motion><ch>`: next key is the delimiter to wrap the resolved range
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
            PendingGrammar::DeleteSurround { count } => {
                self.set_mode(Mode::Normal);
                if let Key::Char(ch) = key {
                    let command = Command::DeleteSurround(ch, count);
                    let result = self.execute_buffer_command(command);
                    if result && !self.dot_repeat.is_replaying() {
                        self.dot_repeat.record_single(command);
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
                    let command = Command::ChangeSurround(from, to, count);
                    let result = self.execute_buffer_command(command);
                    if result && !self.dot_repeat.is_replaying() {
                        self.dot_repeat.record_single(command);
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
                    let command = Command::AddSurround(motion, count, ch, delim_count);
                    let result = self.execute_buffer_command(command);
                    if result && !self.dot_repeat.is_replaying() {
                        self.dot_repeat.record_single(command);
                    }
                }
                self.pending_count = 0;
            }
        }
    }
}
