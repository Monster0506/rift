use super::*;

#[test]
fn test_resolve_input_char() {
    assert_eq!(resolve_input(Key::Char(b'a')), Some(InputIntent::Type('a')));
    assert_eq!(resolve_input(Key::Char(b'Z')), Some(InputIntent::Type('Z')));
    assert_eq!(resolve_input(Key::Char(b' ')), Some(InputIntent::Type(' ')));
    // Non-printable chars should return None (except Tab/Newline handled separately)
    assert_eq!(resolve_input(Key::Char(0)), None);
}

#[test]
fn test_resolve_input_ctrl() {
    // Ctrl+A -> 1
    assert_eq!(
        resolve_input(Key::Ctrl(b'a')),
        Some(InputIntent::Type('\u{1}'))
    );
}

#[test]
fn test_resolve_input_special() {
    assert_eq!(resolve_input(Key::Enter), Some(InputIntent::Accept));
    assert_eq!(resolve_input(Key::Tab), Some(InputIntent::Type('\t')));
    assert_eq!(resolve_input(Key::Escape), Some(InputIntent::Cancel));
}

#[test]
fn test_resolve_input_movement() {
    assert_eq!(
        resolve_input(Key::ArrowLeft),
        Some(InputIntent::Move(Direction::Left, Granularity::Character))
    );
    assert_eq!(
        resolve_input(Key::ArrowRight),
        Some(InputIntent::Move(Direction::Right, Granularity::Character))
    );
    assert_eq!(
        resolve_input(Key::ArrowUp),
        Some(InputIntent::Move(Direction::Up, Granularity::Character))
    );
    assert_eq!(
        resolve_input(Key::ArrowDown),
        Some(InputIntent::Move(Direction::Down, Granularity::Character))
    );
}

#[test]
fn test_resolve_input_word_movement() {
    assert_eq!(
        resolve_input(Key::CtrlArrowLeft),
        Some(InputIntent::Move(Direction::Left, Granularity::Word))
    );
    assert_eq!(
        resolve_input(Key::CtrlArrowRight),
        Some(InputIntent::Move(Direction::Right, Granularity::Word))
    );
}

#[test]
fn test_resolve_input_line_movement() {
    assert_eq!(
        resolve_input(Key::Home),
        Some(InputIntent::Move(Direction::Left, Granularity::Line))
    );
    assert_eq!(
        resolve_input(Key::End),
        Some(InputIntent::Move(Direction::Right, Granularity::Line))
    );
}

#[test]
fn test_resolve_input_page_movement() {
    assert_eq!(
        resolve_input(Key::PageUp),
        Some(InputIntent::Move(Direction::Up, Granularity::Page))
    );
    assert_eq!(
        resolve_input(Key::PageDown),
        Some(InputIntent::Move(Direction::Down, Granularity::Page))
    );
}

#[test]
fn test_resolve_input_deletion() {
    assert_eq!(
        resolve_input(Key::Backspace),
        Some(InputIntent::Delete(Direction::Left, Granularity::Character))
    );
    assert_eq!(
        resolve_input(Key::Delete),
        Some(InputIntent::Delete(
            Direction::Right,
            Granularity::Character
        ))
    );
}
