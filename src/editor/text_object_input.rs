use super::Editor;
use crate::action::{Action, EditorAction, Motion};
use crate::key::Key;
use crate::mode::Mode;
use crate::term::TerminalBackend;
use crate::text_objects::{object_kind_for_key, Direction, Modifier, TextObjectSpec};

/// Accumulates `[direction] [nest-count] object` after a modifier key
/// (`i`/`a`/`I`/`A`) was pressed in `OperatorPending`.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct PendingTextObject {
    modifier: Modifier,
    direction: Direction,
    direction_locked: bool,
    nesting: u8,
}

pub(super) enum TextObjectStep {
    Continue,
    Finalize(TextObjectSpec),
    Cancel,
}

impl PendingTextObject {
    pub(super) fn new(modifier: Modifier) -> Self {
        Self {
            modifier,
            direction: Direction::Current,
            direction_locked: false,
            nesting: 0,
        }
    }

    pub(super) fn advance(&mut self, key: Key) -> TextObjectStep {
        let Key::Char(ch) = key else {
            return TextObjectStep::Cancel;
        };

        if !self.direction_locked && self.nesting == 0 {
            match ch {
                'n' => {
                    self.direction = Direction::Next;
                    self.direction_locked = true;
                    return TextObjectStep::Continue;
                }
                'p' => {
                    self.direction = Direction::Last;
                    self.direction_locked = true;
                    return TextObjectStep::Continue;
                }
                _ => {}
            }
        }

        if ch.is_ascii_digit() && (ch != '0' || self.nesting > 0) {
            let digit = ch.to_digit(10).unwrap() as u8;
            self.nesting = self.nesting.saturating_mul(10).saturating_add(digit);
            return TextObjectStep::Continue;
        }

        match object_kind_for_key(ch) {
            Some(kind) => TextObjectStep::Finalize(TextObjectSpec {
                modifier: self.modifier,
                direction: self.direction,
                nesting: self.nesting.max(1),
                kind,
            }),
            None => TextObjectStep::Cancel,
        }
    }
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn dispatch_text_object_spec(&mut self, spec: TextObjectSpec) {
        self.pending_keys.clear();
        let action = Action::Editor(EditorAction::Move(Motion::TextObject(spec)));
        self.handle_action(&action);
        if self.current_mode != Mode::OperatorPending {
            self.pending_count = 0;
        }
    }
}

#[cfg(test)]
#[path = "text_object_input_tests.rs"]
mod text_object_input_tests;
