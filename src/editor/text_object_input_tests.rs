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
