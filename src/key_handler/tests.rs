//! Tests for key handler

use crate::key::{InputEvent, Key, Mods};
use crate::key_handler::{KeyAction, KeyHandler};
use crate::mode::Mode;

#[test]
fn test_process_normal_mode_debug_toggle() {
    // Debug toggle should return ToggleDebug action
    let action = KeyHandler::process_key(Key::char('?'), Mode::Normal);
    assert_eq!(action, KeyAction::ToggleDebug);
}

#[test]
fn test_process_normal_mode_escape() {
    let action = KeyHandler::process_key(Key::ESC, Mode::Normal);
    assert_eq!(action, KeyAction::SkipAndRender);
}

#[test]
fn test_process_normal_mode_ctrl_bracket() {
    let action = KeyHandler::process_key(Key::ctrl(']'), Mode::Normal);
    assert_eq!(action, KeyAction::SkipAndRender);
}

#[test]
fn test_process_normal_mode_regular_key() {
    let action = KeyHandler::process_key(Key::char('h'), Mode::Normal);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_insert_mode_escape() {
    let action = KeyHandler::process_key(Key::ESC, Mode::Insert);
    assert_eq!(action, KeyAction::ExitInsertMode);
}

#[test]
fn test_process_insert_mode_regular_key() {
    let action = KeyHandler::process_key(Key::char('a'), Mode::Insert);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_insert_mode_debug_toggle_ignored() {
    // Debug toggle should not work in insert mode (should continue to command processing)
    let action = KeyHandler::process_key(Key::char('?'), Mode::Insert);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_normal_mode_arrow_keys() {
    let keys = vec![Key::UP, Key::DOWN, Key::LEFT, Key::RIGHT];

    for key in keys {
        let action = KeyHandler::process_key(key, Mode::Normal);
        assert_eq!(action, KeyAction::Continue);
    }
}

#[test]
fn test_process_insert_mode_special_keys() {
    // Most special keys should continue in insert mode
    let keys = vec![Key::BACKSPACE, Key::DELETE, Key::ENTER, Key::TAB];

    for key in keys {
        let action = KeyHandler::process_key(key, Mode::Insert);
        assert_eq!(action, KeyAction::Continue);
    }
}

#[test]
fn test_process_normal_mode_ctrl_keys() {
    // Ctrl+] should skip, other Ctrl keys should continue
    let action = KeyHandler::process_key(Key::ctrl(']'), Mode::Normal);
    assert_eq!(action, KeyAction::SkipAndRender);

    let action = KeyHandler::process_key(Key::ctrl('c'), Mode::Normal);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_command_mode_escape() {
    // Escape should exit command mode
    let action = KeyHandler::process_key(Key::ESC, Mode::Command);
    assert_eq!(action, KeyAction::ExitCommandMode);
}

#[test]
fn test_process_command_mode_regular_key() {
    // Regular keys should continue in command mode (for future command input)
    let action = KeyHandler::process_key(Key::char('a'), Mode::Command);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_resize_event() {
    // Resize event should return Resize action regardless of mode
    let action = KeyHandler::process_event(InputEvent::Resize(80, 24), Mode::Normal);
    assert_eq!(action, KeyAction::Resize(80, 24));

    let action = KeyHandler::process_event(InputEvent::Resize(100, 30), Mode::Insert);
    assert_eq!(action, KeyAction::Resize(100, 30));

    let action = KeyHandler::process_event(InputEvent::Resize(120, 40), Mode::Command);
    assert_eq!(action, KeyAction::Resize(120, 40));
}

#[test]
fn test_process_event_routes_keys_by_mode() {
    let action = KeyHandler::process_event(InputEvent::Key(Key::ESC), Mode::Insert);
    assert_eq!(action, KeyAction::ExitInsertMode);

    let action = KeyHandler::process_event(InputEvent::Paste("x".into()), Mode::Insert);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_modified_escape_still_exits() {
    let action = KeyHandler::process_key(Key::ESC.with(Mods::ALT), Mode::Insert);
    assert_eq!(action, KeyAction::ExitInsertMode);
}
