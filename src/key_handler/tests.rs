//! Tests for key handler

use crate::key_handler::{KeyHandler, KeyAction};
use crate::key::Key;
use crate::mode::Mode;
use crate::state::State;

#[test]
fn test_process_normal_mode_debug_toggle() {
    let mut state = State::new();
    assert_eq!(state.debug_mode, false);
    
    let action = KeyHandler::process_key(Key::Char(b'?'), Mode::Normal, &mut state);
    assert_eq!(action, KeyAction::SkipAndRender);
    assert_eq!(state.debug_mode, true);
    
    // Toggle again
    let action = KeyHandler::process_key(Key::Char(b'?'), Mode::Normal, &mut state);
    assert_eq!(action, KeyAction::SkipAndRender);
    assert_eq!(state.debug_mode, false);
}

#[test]
fn test_process_normal_mode_escape() {
    let mut state = State::new();
    let action = KeyHandler::process_key(Key::Escape, Mode::Normal, &mut state);
    assert_eq!(action, KeyAction::SkipAndRender);
}

#[test]
fn test_process_normal_mode_ctrl_bracket() {
    let mut state = State::new();
    let action = KeyHandler::process_key(Key::Ctrl(b']'), Mode::Normal, &mut state);
    assert_eq!(action, KeyAction::SkipAndRender);
}

#[test]
fn test_process_normal_mode_regular_key() {
    let mut state = State::new();
    let action = KeyHandler::process_key(Key::Char(b'h'), Mode::Normal, &mut state);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_insert_mode_escape() {
    let mut state = State::new();
    let action = KeyHandler::process_key(Key::Escape, Mode::Insert, &mut state);
    assert_eq!(action, KeyAction::ExitInsertMode);
}

#[test]
fn test_process_insert_mode_regular_key() {
    let mut state = State::new();
    let action = KeyHandler::process_key(Key::Char(b'a'), Mode::Insert, &mut state);
    assert_eq!(action, KeyAction::Continue);
}

#[test]
fn test_process_insert_mode_debug_toggle_ignored() {
    let mut state = State::new();
    // Debug toggle should not work in insert mode
    let action = KeyHandler::process_key(Key::Char(b'?'), Mode::Insert, &mut state);
    assert_eq!(action, KeyAction::Continue);
    assert_eq!(state.debug_mode, false);
}

#[test]
fn test_process_normal_mode_arrow_keys() {
    let mut state = State::new();
    let keys = vec![
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
    ];
    
    for key in keys {
        let action = KeyHandler::process_key(key, Mode::Normal, &mut state);
        assert_eq!(action, KeyAction::Continue);
    }
}

#[test]
fn test_process_insert_mode_special_keys() {
    let mut state = State::new();
    // Most special keys should continue in insert mode
    let keys = vec![
        Key::Backspace,
        Key::Delete,
        Key::Enter,
        Key::Tab,
    ];
    
    for key in keys {
        let action = KeyHandler::process_key(key, Mode::Insert, &mut state);
        assert_eq!(action, KeyAction::Continue);
    }
}

#[test]
fn test_process_normal_mode_ctrl_keys() {
    let mut state = State::new();
    // Ctrl+] should skip, other Ctrl keys should continue
    let action = KeyHandler::process_key(Key::Ctrl(b']'), Mode::Normal, &mut state);
    assert_eq!(action, KeyAction::SkipAndRender);
    
    let action = KeyHandler::process_key(Key::Ctrl(b'c'), Mode::Normal, &mut state);
    assert_eq!(action, KeyAction::Continue);
}

