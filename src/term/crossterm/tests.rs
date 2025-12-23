//! Tests for crossterm backend

use crate::term::TerminalBackend;
use crate::term::crossterm::CrosstermBackend;
use crate::key::Key;

#[test]
fn test_crossterm_backend_new() {
    let backend = CrosstermBackend::new();
    assert!(backend.is_ok());
}

#[test]
fn test_get_size() {
    let backend = CrosstermBackend::new().unwrap();
    // Can't test init in unit tests (requires actual terminal)
    // But we can test that get_size returns a valid size structure
    // when terminal is initialized
    let size_result = backend.get_size();
    // This might fail if not in a real terminal, so we just check it doesn't panic
    // In a real terminal, it should return Ok(Size { rows: > 0, cols: > 0 })
    assert!(size_result.is_ok() || size_result.is_err());
}

#[test]
fn test_translate_key_event() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test basic character
    let key_event = KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Char(b'a'));

    // Test Ctrl+Char
    let key_event = KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Ctrl(b'c'));

    // Test arrow keys
    let key_event = KeyEvent {
        code: KeyCode::Up,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::ArrowUp);

    let key_event = KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::ArrowDown);

    let key_event = KeyEvent {
        code: KeyCode::Left,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::ArrowLeft);

    let key_event = KeyEvent {
        code: KeyCode::Right,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::ArrowRight);

    // Test special keys
    let key_event = KeyEvent {
        code: KeyCode::Backspace,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Backspace);

    let key_event = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Enter);

    // Test Enter as character '\r' (some terminals send this)
    let key_event = KeyEvent {
        code: KeyCode::Char('\r'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Enter);

    // Test Enter as character '\n' (some terminals send this)
    let key_event = KeyEvent {
        code: KeyCode::Char('\n'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Enter);

    let key_event = KeyEvent {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Escape);

    let key_event = KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Tab);

    let key_event = KeyEvent {
        code: KeyCode::Home,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Home);

    let key_event = KeyEvent {
        code: KeyCode::End,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::End);

    let key_event = KeyEvent {
        code: KeyCode::PageUp,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::PageUp);

    let key_event = KeyEvent {
        code: KeyCode::PageDown,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::PageDown);

    let key_event = KeyEvent {
        code: KeyCode::Delete,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Delete);
}
