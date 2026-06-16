use super::Editor;
use crate::action::{Action, EditorAction, Motion};
use crate::key::Key;
use crate::mode::Mode;
#[allow(unused_imports)]
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
mod tests {
    use super::*;
    use crate::text_objects::ObjectKind;

    fn finalize(keys: &str, modifier: Modifier) -> TextObjectSpec {
        let mut pending = PendingTextObject::new(modifier);
        let mut keys = keys.chars().peekable();
        loop {
            let ch = keys.next().expect("ran out of keys before Finalize");
            match pending.advance(Key::Char(ch)) {
                TextObjectStep::Continue => continue,
                TextObjectStep::Finalize(spec) => return spec,
                TextObjectStep::Cancel => panic!("unexpected cancel on key '{ch}'"),
            }
        }
    }

    #[test]
    fn plain_object_key_defaults_direction_and_nesting() {
        let spec = finalize("w", Modifier::Inner);
        assert_eq!(spec.direction, Direction::Current);
        assert_eq!(spec.nesting, 1);
        assert_eq!(spec.kind, ObjectKind::Word);
    }

    #[test]
    fn n_prefix_locks_next_direction() {
        let spec = finalize("n(", Modifier::Around);
        assert_eq!(spec.direction, Direction::Next);
        assert_eq!(spec.kind, ObjectKind::Paren);
    }

    #[test]
    fn p_prefix_locks_last_direction() {
        let spec = finalize("p(", Modifier::Around);
        assert_eq!(spec.direction, Direction::Last);
    }

    #[test]
    fn digits_accumulate_into_nesting() {
        let spec = finalize("12(", Modifier::Inner);
        assert_eq!(spec.nesting, 12);
    }

    #[test]
    fn direction_then_nesting_then_object() {
        let spec = finalize("n3(", Modifier::Inner);
        assert_eq!(spec.direction, Direction::Next);
        assert_eq!(spec.nesting, 3);
    }

    #[test]
    fn leading_zero_is_not_a_count_digit() {
        // A leading '0' is not a valid nest-count digit (matches vim's 0 == "start
        // of line" convention) and isn't an object key either, so it cancels.
        let mut pending = PendingTextObject::new(Modifier::Inner);
        assert!(matches!(
            pending.advance(Key::Char('0')),
            TextObjectStep::Cancel
        ));
    }

    #[test]
    fn unknown_object_key_cancels() {
        let mut pending = PendingTextObject::new(Modifier::Inner);
        assert!(matches!(
            pending.advance(Key::Char('z')),
            TextObjectStep::Cancel
        ));
    }

    #[test]
    fn non_char_key_cancels() {
        let mut pending = PendingTextObject::new(Modifier::Inner);
        assert!(matches!(
            pending.advance(Key::Escape),
            TextObjectStep::Cancel
        ));
    }
}
