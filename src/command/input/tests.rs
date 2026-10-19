use super::*;

#[test]
fn test_resolve_input_char() {
    assert_eq!(resolve_input(Key::char('a')), Some(InputIntent::Type('a')));
    assert_eq!(resolve_input(Key::char('Z')), Some(InputIntent::Type('Z')));
    assert_eq!(resolve_input(Key::char(' ')), Some(InputIntent::Type(' ')));
    // Unicode characters
    assert_eq!(
        resolve_input(Key::char('🦀')),
        Some(InputIntent::Type('🦀'))
    );
    // Non-printable chars should return None (except Tab/Newline handled separately)
    assert_eq!(resolve_input(Key::char('\0')), None);
}

#[test]
fn test_resolve_input_ctrl() {
    // Ctrl+A -> 1
    assert_eq!(
        resolve_input(Key::ctrl('a')),
        Some(InputIntent::Type('\u{1}'))
    );
}

#[test]
fn test_resolve_input_special() {
    assert_eq!(resolve_input(Key::ENTER), Some(InputIntent::Accept));
    assert_eq!(resolve_input(Key::TAB), Some(InputIntent::Type('\t')));
    assert_eq!(resolve_input(Key::ESC), Some(InputIntent::Cancel));
}

#[test]
fn test_resolve_input_movement() {
    assert_eq!(
        resolve_input(Key::LEFT),
        Some(InputIntent::Move(Direction::Left, Granularity::Character))
    );
    assert_eq!(
        resolve_input(Key::RIGHT),
        Some(InputIntent::Move(Direction::Right, Granularity::Character))
    );
    assert_eq!(
        resolve_input(Key::UP),
        Some(InputIntent::Move(Direction::Up, Granularity::Character))
    );
    assert_eq!(
        resolve_input(Key::DOWN),
        Some(InputIntent::Move(Direction::Down, Granularity::Character))
    );
}

#[test]
fn test_resolve_input_word_movement() {
    assert_eq!(
        resolve_input(Key::LEFT.with(Mods::CTRL)),
        Some(InputIntent::Move(Direction::Left, Granularity::Word))
    );
    assert_eq!(
        resolve_input(Key::RIGHT.with(Mods::CTRL)),
        Some(InputIntent::Move(Direction::Right, Granularity::Word))
    );
}

#[test]
fn test_resolve_input_line_movement() {
    assert_eq!(
        resolve_input(Key::HOME),
        Some(InputIntent::Move(Direction::Left, Granularity::Line))
    );
    assert_eq!(
        resolve_input(Key::END),
        Some(InputIntent::Move(Direction::Right, Granularity::Line))
    );
}

#[test]
fn test_resolve_input_page_movement() {
    assert_eq!(
        resolve_input(Key::PAGE_UP),
        Some(InputIntent::Move(Direction::Up, Granularity::Page))
    );
    assert_eq!(
        resolve_input(Key::PAGE_DOWN),
        Some(InputIntent::Move(Direction::Down, Granularity::Page))
    );
}

#[test]
fn test_resolve_input_deletion() {
    assert_eq!(
        resolve_input(Key::BACKSPACE),
        Some(InputIntent::Delete(Direction::Left, Granularity::Character))
    );
    assert_eq!(
        resolve_input(Key::DELETE),
        Some(InputIntent::Delete(
            Direction::Right,
            Granularity::Character
        ))
    );
}

#[test]
fn test_resolve_input_ctrl_home_end_is_document() {
    assert_eq!(
        resolve_input(Key::HOME.with(Mods::CTRL)),
        Some(InputIntent::Move(Direction::Left, Granularity::Document))
    );
    assert_eq!(
        resolve_input(Key::END.with(Mods::CTRL)),
        Some(InputIntent::Move(Direction::Right, Granularity::Document))
    );
}

#[test]
fn test_resolve_input_shift_space_types_space() {
    assert_eq!(
        resolve_input(Key::SHIFT_SPACE),
        Some(InputIntent::Type(' '))
    );
}

#[test]
fn test_resolve_input_alt_and_function_keys_are_not_input() {
    assert_eq!(resolve_input(Key::alt('x')), None);
    assert_eq!(resolve_input(Key::f(5)), None);
    assert_eq!(resolve_input(Key::SHIFT_TAB), None);
}
