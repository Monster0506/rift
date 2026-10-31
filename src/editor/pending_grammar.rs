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
                        let count = if self.pending_count > 0 {
                            self.pending_count
                        } else {
                            1
                        };
                        let command = Command::ReplaceChar(ch, count);
                        self.execute_and_record(command);
                    }
                }
                self.pending_count = 0;
            }
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
            PendingGrammar::SurroundVerb { count } => self.advance_surround_verb(count, key),
            PendingGrammar::DeleteSurround { count } => {
                self.set_mode(Mode::Normal);
                if let Some(ch) = key.text() {
                    if self.try_run_set_aware_delete_surround(ch, count) {
                        self.finish_region_build(None);
                    } else {
                        let command = Command::DeleteSurround(ch, count);
                        self.execute_and_record(command);
                    }
                }
                self.pending_count = 0;
            }
            PendingGrammar::ChangeSurroundFrom { count } => {
                if let Some(from) = key.text() {
                    self.pending_grammar = Some(PendingGrammar::ChangeSurroundTo { from, count });
                } else {
                    self.set_mode(Mode::Normal);
                    self.pending_count = 0;
                }
            }
            PendingGrammar::ChangeSurroundTo { from, count } => {
                self.set_mode(Mode::Normal);
                if let Some(to) = key.text() {
                    if self.try_run_set_aware_change_surround(from, to, count) {
                        self.finish_region_build(None);
                    } else {
                        let command = Command::ChangeSurround(from, to, count);
                        self.execute_and_record(command);
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
                if let Some(ch) = key.text() {
                    if self.try_run_set_aware_add_surround(ch, delim_count) {
                        self.finish_region_build(Some(Action::Editor(
                            EditorAction::AddSurroundToSet { ch, delim_count },
                        )));
                    } else {
                        let command = Command::AddSurround(motion, count, ch, delim_count);
                        self.execute_and_record(command);
                    }
                }
                self.pending_count = 0;
            }
        }
    }

    pub(super) fn replace_selection(&mut self, ch: char) -> bool {
        let was_visual = self.current_mode.is_visual();
        let has_regions = self
            .document_manager
            .active_document()
            .is_some_and(|doc| !doc.selection_set.is_empty());
        if !was_visual && !has_regions {
            return false;
        }
        if self.active_doc_is(crate::document::Document::is_read_only) {
            return true;
        }

        let start = self.visual_anchor.and_then(|anchor| {
            self.document_manager
                .active_document()
                .map(|doc| anchor.min(doc.buffer.cursor()).min(doc.buffer.len()))
        });
        if was_visual {
            self.set_mode(Mode::Normal);
        }
        if let Some(start) = start {
            if let Some(doc) = self.document_manager.active_document_mut() {
                let _ = doc.buffer.set_cursor(start);
            }
        }
        let result = self.try_run_set_aware_replace_char(ch);
        if let Some(start) = start {
            if let Some(doc) = self.document_manager.active_document_mut() {
                let _ = doc.buffer.set_cursor(start);
            }
        }
        let follow_up = (!was_visual).then(|| Action::Editor(EditorAction::ReplaceSelection(ch)));
        self.finish_region_build(follow_up);
        result || was_visual
    }

    fn advance_surround_verb(&mut self, count: usize, key: Key) {
        match key.text() {
            Some('d') => {
                let count = if count > 0 { count } else { 1 };
                self.pending_grammar = Some(PendingGrammar::DeleteSurround { count });
            }
            Some('c') => {
                let count = if count > 0 { count } else { 1 };
                self.pending_grammar = Some(PendingGrammar::ChangeSurroundFrom { count });
            }
            Some('g') => {
                let delim_count = if count > 0 { count } else { 1 };
                self.pending_operator = Some(super::PendingOperator::SurroundAdd(delim_count));
                self.set_mode(Mode::OperatorPending);
            }
            _ => {
                self.set_mode(Mode::Normal);
            }
        }
        self.pending_count = 0;
    }

    fn execute_and_record(&mut self, command: Command) {
        let result = self.execute_buffer_command(command);
        if result && !self.dot_repeat.is_replaying() {
            self.dot_repeat.record_single(command);
        }
    }
}
