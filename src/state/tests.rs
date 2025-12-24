//! Tests for state management

use crate::state::{State, UserSettings};
use crate::key::Key;

#[test]
fn test_state_new() {
    let state = State::new();
    assert_eq!(state.debug_mode, false);
    assert_eq!(state.last_keypress, None);
    assert_eq!(state.cursor_pos, (0, 0));
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);
}

#[test]
fn test_state_default() {
    let state = State::default();
    assert_eq!(state.debug_mode, false);
    assert_eq!(state.last_keypress, None);
    assert_eq!(state.cursor_pos, (0, 0));
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);
}

#[test]
fn test_toggle_debug() {
    let mut state = State::new();
    assert_eq!(state.debug_mode, false);
    
    // Toggle on
    state.toggle_debug();
    assert_eq!(state.debug_mode, true);
    
    // Toggle off
    state.toggle_debug();
    assert_eq!(state.debug_mode, false);
    
    // Toggle on again
    state.toggle_debug();
    assert_eq!(state.debug_mode, true);
}

#[test]
fn test_update_keypress() {
    let mut state = State::new();
    assert_eq!(state.last_keypress, None);
    
    // Update with a character key
    state.update_keypress(Key::Char(b'a'));
    assert_eq!(state.last_keypress, Some(Key::Char(b'a')));
    
    // Update with another key
    state.update_keypress(Key::Char(b'b'));
    assert_eq!(state.last_keypress, Some(Key::Char(b'b')));
    
    // Update with arrow key
    state.update_keypress(Key::ArrowUp);
    assert_eq!(state.last_keypress, Some(Key::ArrowUp));
    
    // Update with Ctrl key
    state.update_keypress(Key::Ctrl(b'c'));
    assert_eq!(state.last_keypress, Some(Key::Ctrl(b'c')));
}

#[test]
fn test_update_cursor() {
    let mut state = State::new();
    assert_eq!(state.cursor_pos, (0, 0));
    
    // Update cursor position
    state.update_cursor(5, 10);
    assert_eq!(state.cursor_pos, (5, 10));
    
    // Update to different position
    state.update_cursor(0, 0);
    assert_eq!(state.cursor_pos, (0, 0));
    
    // Update to large values
    state.update_cursor(100, 200);
    assert_eq!(state.cursor_pos, (100, 200));
}

#[test]
fn test_update_buffer_stats() {
    let mut state = State::new();
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);
    
    // Update buffer stats
    state.update_buffer_stats(10, 500);
    assert_eq!(state.total_lines, 10);
    assert_eq!(state.buffer_size, 500);
    
    // Update to different values
    state.update_buffer_stats(1, 0);
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);
    
    // Update to large values
    state.update_buffer_stats(1000, 50000);
    assert_eq!(state.total_lines, 1000);
    assert_eq!(state.buffer_size, 50000);
}

#[test]
fn test_state_operations_together() {
    let mut state = State::new();
    
    // Set up initial state
    state.update_keypress(Key::Char(b'h'));
    state.update_cursor(2, 5);
    state.update_buffer_stats(3, 100);
    assert_eq!(state.debug_mode, false);
    assert_eq!(state.last_keypress, Some(Key::Char(b'h')));
    assert_eq!(state.cursor_pos, (2, 5));
    assert_eq!(state.total_lines, 3);
    assert_eq!(state.buffer_size, 100);
    
    // Toggle debug mode
    state.toggle_debug();
    assert_eq!(state.debug_mode, true);
    
    // Update all fields
    state.update_keypress(Key::ArrowDown);
    state.update_cursor(10, 20);
    state.update_buffer_stats(15, 200);
    
    assert_eq!(state.debug_mode, true);
    assert_eq!(state.last_keypress, Some(Key::ArrowDown));
    assert_eq!(state.cursor_pos, (10, 20));
    assert_eq!(state.total_lines, 15);
    assert_eq!(state.buffer_size, 200);
}

#[test]
fn test_multiple_keypress_updates() {
    let mut state = State::new();
    
    // Simulate a sequence of keypresses
    let keys = vec![
        Key::Char(b'i'),
        Key::Char(b'n'),
        Key::Char(b's'),
        Key::Char(b'e'),
        Key::Char(b'r'),
        Key::Char(b't'),
        Key::Escape,
    ];
    
    for key in keys {
        state.update_keypress(key);
    }
    
    // Last keypress should be Escape
    assert_eq!(state.last_keypress, Some(Key::Escape));
}

#[test]
fn test_cursor_position_updates() {
    let mut state = State::new();
    
    // Simulate cursor movement
    state.update_cursor(0, 0);
    assert_eq!(state.cursor_pos, (0, 0));
    
    state.update_cursor(0, 1);
    assert_eq!(state.cursor_pos, (0, 1));
    
    state.update_cursor(0, 2);
    assert_eq!(state.cursor_pos, (0, 2));
    
    // Move to next line
    state.update_cursor(1, 0);
    assert_eq!(state.cursor_pos, (1, 0));
}

#[test]
fn test_buffer_stats_updates() {
    let mut state = State::new();
    
    // Simulate buffer growth
    state.update_buffer_stats(1, 0);
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);
    
    state.update_buffer_stats(1, 5);
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 5);
    
    state.update_buffer_stats(2, 10);
    assert_eq!(state.total_lines, 2);
    assert_eq!(state.buffer_size, 10);
    
    state.update_buffer_stats(3, 15);
    assert_eq!(state.total_lines, 3);
    assert_eq!(state.buffer_size, 15);
}

#[test]
fn test_user_settings_default() {
    let settings = UserSettings::default();
    assert_eq!(settings.expand_tabs, true);
    assert_eq!(settings.tab_width, 4);
    assert_eq!(settings.default_border_chars, None);
    assert_eq!(settings.command_line_window.width_ratio, 0.6);
    assert_eq!(settings.command_line_window.min_width, 40);
    assert_eq!(settings.command_line_window.height, 3);
    assert_eq!(settings.command_line_window.border, true);
    assert_eq!(settings.command_line_window.reverse_video, false);
}

#[test]
fn test_state_with_custom_settings() {
    let mut custom_settings = UserSettings::new();
    custom_settings.expand_tabs = false;
    custom_settings.tab_width = 4;
    
    let state = State::with_settings(custom_settings);
    assert_eq!(state.settings.expand_tabs, false);
    assert_eq!(state.settings.tab_width, 4);
    assert_eq!(state.debug_mode, false); // Runtime state should still be default
}

#[test]
fn test_set_expand_tabs() {
    let mut state = State::new();
    assert_eq!(state.settings.expand_tabs, true);
    
    state.set_expand_tabs(false);
    assert_eq!(state.settings.expand_tabs, false);
    
    state.set_expand_tabs(true);
    assert_eq!(state.settings.expand_tabs, true);
}

#[test]
fn test_set_default_border_chars() {
    use crate::floating_window::BorderChars;
    
    let mut state = State::new();
    assert_eq!(state.settings.default_border_chars, None);
    
    let border_chars = BorderChars {
        top_left: vec![b'+'],
        top_right: vec![b'+'],
        bottom_left: vec![b'+'],
        bottom_right: vec![b'+'],
        horizontal: vec![b'-'],
        vertical: vec![b'|'],
    };
    
    state.set_default_border_chars(Some(border_chars));
    assert!(state.settings.default_border_chars.is_some());
    
    state.set_default_border_chars(None);
    assert_eq!(state.settings.default_border_chars, None);
}

