//! Tests for key handler

use crate::key_handler::{KeyHandler, KeyAction};
use crate::key::Key;
use crate::mode::Mode;

#[test]
fn test_process_normal_mode_debug_toggle() {
    // Debug toggle should return ToggleDebug action
    let action = KeyHandler::process_key(Key::Char(b'?'), Mode::Normal);
    assert_eq!(action, KeyAction::ToggleDebug);
}

#[test]
fn test_process_normal_mode_escape() {
    let action = KeyHandler::process_key(Key::Escape, Mode::Normal);
    assert_eq!(action, KeyAction::SkipAndRender);
}

#[test]
fn test_process_normal_mode_ctrl_bracket() {
    let action = KeyHandler::process_key(Key::Ctrl(b']'), Mode::Normal);
    assert_eq!(action, KeyAction::SkipAndRender);
}

#[test]
fn test_process_normal_mode_regular_key() {
    let action = KeyHandler::process_key(Key::Char(b'h'), Mode::Normal);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_insert_mode_escape() {
    let action = KeyHandler::process_key(Key::Escape, Mode::Insert);
    assert_eq!(action, KeyAction::ExitInsertMode);
}

#[test]
fn test_process_insert_mode_regular_key() {
    let action = KeyHandler::process_key(Key::Char(b'a'), Mode::Insert);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_insert_mode_debug_toggle_ignored() {
    // Debug toggle should not work in insert mode (should continue to command processing)
    let action = KeyHandler::process_key(Key::Char(b'?'), Mode::Insert);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_normal_mode_arrow_keys() {
    let keys = vec![
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
    ];
    
    for key in keys {
        let action = KeyHandler::process_key(key, Mode::Normal);
        assert_eq!(action, KeyAction::Continue);
    }
}

#[test]
fn test_process_insert_mode_special_keys() {
    // Most special keys should continue in insert mode
    let keys = vec![
        Key::Backspace,
        Key::Delete,
        Key::Enter,
        Key::Tab,
    ];
    
    for key in keys {
        let action = KeyHandler::process_key(key, Mode::Insert);
        assert_eq!(action, KeyAction::Continue);
    }
}

#[test]
fn test_process_normal_mode_ctrl_keys() {
    // Ctrl+] should skip, other Ctrl keys should continue
    let action = KeyHandler::process_key(Key::Ctrl(b']'), Mode::Normal);
    assert_eq!(action, KeyAction::SkipAndRender);
    
    let action = KeyHandler::process_key(Key::Ctrl(b'c'), Mode::Normal);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_command_mode_escape() {
    // Escape should exit command mode
    let action = KeyHandler::process_key(Key::Escape, Mode::Command);
    assert_eq!(action, KeyAction::ExitCommandMode);
}

#[test]
fn test_process_command_mode_regular_key() {
    // Regular keys should continue in command mode (for future command input)
    let action = KeyHandler::process_key(Key::Char(b'a'), Mode::Command);
    assert_eq!(action, KeyAction::Continue);
}

