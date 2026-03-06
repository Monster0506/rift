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

// ──────────────────────────────────────────────
// FileExplorer → Normal → Global fallback chain
// ──────────────────────────────────────────────

#[test]
fn test_file_explorer_buffer_falls_back_to_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    // FileExplorer should find Normal bindings via fallback
    assert_eq!(
        map.get_action(KeyContext::FileExplorer, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
}

#[test]
fn test_file_explorer_buffer_falls_back_to_global_via_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Char('q'),
        Action::Editor(EditorAction::Quit),
    );

    // FileExplorer → Normal → Global
    assert_eq!(
        map.get_action(KeyContext::FileExplorer, Key::Char('q')),
        Some(&Action::Editor(EditorAction::Quit))
    );
}

#[test]
fn test_file_explorer_buffer_override_shadows_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Normal,
        Key::Char('k'),
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    map.register(
        KeyContext::FileExplorer,
        Key::Char('k'),
        Action::Editor(EditorAction::Move(Motion::PageUp)),
    );

    // FileExplorer override wins
    assert_eq!(
        map.get_action(KeyContext::FileExplorer, Key::Char('k')),
        Some(&Action::Editor(EditorAction::Move(Motion::PageUp)))
    );
    // Normal still sees its own binding
    assert_eq!(
        map.get_action(KeyContext::Normal, Key::Char('k')),
        Some(&Action::Editor(EditorAction::Move(Motion::Up)))
    );
}

#[test]
fn test_undotree_falls_back_to_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    assert_eq!(
        map.get_action(KeyContext::UndoTree, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
}

#[test]
fn test_undotree_falls_back_to_global_via_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Escape,
        Action::Editor(EditorAction::EnterNormalMode),
    );

    assert_eq!(
        map.get_action(KeyContext::UndoTree, Key::Escape),
        Some(&Action::Editor(EditorAction::EnterNormalMode))
    );
}

#[test]
fn test_undotree_override_shadows_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Normal,
        Key::Enter,
        Action::Noop,
    );
    map.register(
        KeyContext::UndoTree,
        Key::Enter,
        Action::Buffer("undotree:select".to_string()),
    );

    assert_eq!(
        map.get_action(KeyContext::UndoTree, Key::Enter),
        Some(&Action::Buffer("undotree:select".to_string()))
    );
}

#[test]
fn test_normal_does_not_see_file_explorer_buffer_bindings() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::FileExplorer,
        Key::Enter,
        Action::Buffer("explorer:select".to_string()),
    );

    // Normal does NOT fall back to FileExplorer
    assert_eq!(
        map.get_action(KeyContext::Normal, Key::Enter),
        None
    );
}

#[test]
fn test_normal_does_not_see_undotree_bindings() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::UndoTree,
        Key::Enter,
        Action::Buffer("undotree:select".to_string()),
    );

    assert_eq!(
        map.get_action(KeyContext::Normal, Key::Enter),
        None
    );
}

#[test]
fn test_file_explorer_buffer_all_normal_motions_accessible() {
    let mut map = KeyMap::new();
    let motions = vec![
        (Key::Char('h'), Motion::Left),
        (Key::Char('j'), Motion::Down),
        (Key::Char('k'), Motion::Up),
        (Key::Char('l'), Motion::Right),
    ];
    for (key, motion) in &motions {
        map.register(
            KeyContext::Normal,
            *key,
            Action::Editor(EditorAction::Move(*motion)),
        );
    }

    for (key, motion) in &motions {
        assert_eq!(
            map.get_action(KeyContext::FileExplorer, *key),
            Some(&Action::Editor(EditorAction::Move(*motion))),
            "FileExplorer should inherit {:?} from Normal", key
        );
    }
}

#[test]
fn test_undotree_all_normal_motions_accessible() {
    let mut map = KeyMap::new();
    let motions = vec![
        (Key::Char('h'), Motion::Left),
        (Key::Char('j'), Motion::Down),
        (Key::Char('k'), Motion::Up),
        (Key::Char('l'), Motion::Right),
    ];
    for (key, motion) in &motions {
        map.register(
            KeyContext::Normal,
            *key,
            Action::Editor(EditorAction::Move(*motion)),
        );
    }

    for (key, motion) in &motions {
        assert_eq!(
            map.get_action(KeyContext::UndoTree, *key),
            Some(&Action::Editor(EditorAction::Move(*motion))),
            "UndoTree should inherit {:?} from Normal", key
        );
    }
}


#[test]
fn test_sequence_fallback_through_normal_to_global() {
    let mut map = KeyMap::new();
    map.register_sequence(
        KeyContext::Global,
        vec![Key::Char('g'), Key::Char('g')],
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );

    // FileExplorer → Normal → Global should find 'gg'
    assert_eq!(
        map.lookup(KeyContext::FileExplorer, &[Key::Char('g'), Key::Char('g')]),
        MatchResult::Exact(&Action::Editor(EditorAction::Move(Motion::StartOfFile)))
    );
}

#[test]
fn test_sequence_fallback_undotree_through_normal_to_global() {
    let mut map = KeyMap::new();
    map.register_sequence(
        KeyContext::Global,
        vec![Key::Char('g'), Key::Char('g')],
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );

    assert_eq!(
        map.lookup(KeyContext::UndoTree, &[Key::Char('g'), Key::Char('g')]),
        MatchResult::Exact(&Action::Editor(EditorAction::Move(Motion::StartOfFile)))
    );
}
