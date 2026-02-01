use super::*;
use crate::action::{EditorAction, Motion};

#[test]
fn test_register_and_get() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
    assert_eq!(map.get_action(KeyContext::Global, Key::Char('k')), None);
}

#[test]
fn test_sequence() {
    let mut map = KeyMap::new();
    map.register_sequence(
        KeyContext::Global,
        vec![Key::Char('d'), Key::Char('d')],
        Action::Editor(EditorAction::DeleteLine),
    );

    // Partial match
    assert_eq!(
        map.lookup(KeyContext::Global, &[Key::Char('d')]),
        MatchResult::Prefix
    );

    // Exact match
    assert_eq!(
        map.lookup(KeyContext::Global, &[Key::Char('d'), Key::Char('d')]),
        MatchResult::Exact(&Action::Editor(EditorAction::DeleteLine))
    );

    // No match
    assert_eq!(
        map.lookup(KeyContext::Global, &[Key::Char('x')]),
        MatchResult::None
    );
}

#[test]
fn test_context_fallback() {
    let mut map = KeyMap::new();
    // Global binding
    map.register(
        KeyContext::Global,
        Key::Char('q'),
        Action::Editor(EditorAction::Quit),
    );

    // Specific binding
    map.register(
        KeyContext::FileExplorer,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    // Test specific context finding specific binding
    assert_eq!(
        map.get_action(KeyContext::FileExplorer, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );

    // Test specific context falling back to global
    assert_eq!(
        map.get_action(KeyContext::FileExplorer, Key::Char('q')),
        Some(&Action::Editor(EditorAction::Quit))
    );

    // Test global context finding global binding
    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('q')),
        Some(&Action::Editor(EditorAction::Quit))
    );

    // Test global context NOT finding specific binding
    assert_eq!(map.get_action(KeyContext::Global, Key::Char('j')), None);
}

#[test]
fn test_overwrite() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );

    map.register(KeyContext::Global, Key::Char('j'), Action::Noop);
    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('j')),
        Some(&Action::Noop)
    );
}

#[test]
fn test_register_from_str() {
    let mut map = KeyMap::new();
    map.register_from_str(KeyContext::Global, Key::Char('j'), "editor:move_down");
    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
}
