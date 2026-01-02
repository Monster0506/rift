//! Tests for state management

use crate::key::Key;
use crate::state::{State, UserSettings};

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

    state.update_keypress(Key::Char('a'));
    assert_eq!(state.last_keypress, Some(Key::Char('a')));

    state.update_keypress(Key::Char('b'));
    assert_eq!(state.last_keypress, Some(Key::Char('b')));

    state.update_keypress(Key::ArrowUp);
    assert_eq!(state.last_keypress, Some(Key::ArrowUp));

    state.update_keypress(Key::Ctrl(b'c'));
    assert_eq!(state.last_keypress, Some(Key::Ctrl(b'c')));
}

#[test]
fn test_update_cursor() {
    let mut state = State::new();
    assert_eq!(state.cursor_pos, (0, 0));

    state.update_cursor(5, 10);
    assert_eq!(state.cursor_pos, (5, 10));

    state.update_cursor(0, 0);
    assert_eq!(state.cursor_pos, (0, 0));

    state.update_cursor(100, 200);
    assert_eq!(state.cursor_pos, (100, 200));
}

#[test]
fn test_update_buffer_stats() {
    let mut state = State::new();
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);

    state.update_buffer_stats(10, 500, crate::document::LineEnding::LF);
    assert_eq!(state.total_lines, 10);
    assert_eq!(state.buffer_size, 500);

    state.update_buffer_stats(1, 0, crate::document::LineEnding::LF);
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);

    state.update_buffer_stats(1000, 50000, crate::document::LineEnding::LF);
    assert_eq!(state.total_lines, 1000);
    assert_eq!(state.buffer_size, 50000);
}

#[test]
fn test_state_operations_together() {
    let mut state = State::new();

    state.update_keypress(Key::Char('h'));
    state.update_cursor(2, 5);
    state.update_buffer_stats(3, 100, crate::document::LineEnding::LF);
    assert_eq!(state.debug_mode, false);
    assert_eq!(state.last_keypress, Some(Key::Char('h')));
    assert_eq!(state.cursor_pos, (2, 5));
    assert_eq!(state.total_lines, 3);
    assert_eq!(state.buffer_size, 100);

    state.toggle_debug();
    assert_eq!(state.debug_mode, true);

    state.update_keypress(Key::ArrowDown);
    state.update_cursor(10, 20);
    state.update_buffer_stats(15, 200, crate::document::LineEnding::LF);

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
        Key::Char('i'),
        Key::Char('n'),
        Key::Char('s'),
        Key::Char('e'),
        Key::Char('r'),
        Key::Char('t'),
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

    state.update_cursor(0, 0);
    assert_eq!(state.cursor_pos, (0, 0));

    state.update_cursor(0, 1);
    assert_eq!(state.cursor_pos, (0, 1));

    state.update_cursor(0, 2);
    assert_eq!(state.cursor_pos, (0, 2));

    state.update_cursor(1, 0);
    assert_eq!(state.cursor_pos, (1, 0));
}

#[test]
fn test_buffer_stats_updates() {
    let mut state = State::new();

    // Simulate buffer growth
    state.update_buffer_stats(1, 0, crate::document::LineEnding::LF);
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);

    state.update_buffer_stats(1, 5, crate::document::LineEnding::LF);
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 5);

    state.update_buffer_stats(2, 10, crate::document::LineEnding::LF);
    assert_eq!(state.total_lines, 2);
    assert_eq!(state.buffer_size, 10);

    state.update_buffer_stats(3, 15, crate::document::LineEnding::LF);
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
        top_left: '+',
        top_right: '+',
        bottom_left: '+',
        bottom_right: '+',
        horizontal: '-',
        vertical: '|',
    };

    state.set_default_border_chars(Some(border_chars));
    assert!(state.settings.default_border_chars.is_some());

    state.set_default_border_chars(None);
    assert_eq!(state.settings.default_border_chars, None);
}
#[test]
fn test_handle_error() {
    use crate::error::{ErrorType, RiftError};
    use crate::notification::NotificationType;

    let mut state = State::new();
    let err = RiftError::new(ErrorType::Io, "E1", "io failure");

    state.handle_error(err.clone());

    // Verify notification is added
    let notifications: Vec<_> = state.error_manager.notifications().iter_active().collect();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].message, "io failure");
    assert_eq!(notifications[0].kind, NotificationType::Error);
}

#[test]
fn test_handle_error_severity_mapping() {
    use crate::error::{ErrorSeverity, ErrorType, RiftError};
    use crate::notification::NotificationType;

    let mut state = State::new();

    // Warning
    let warn = RiftError::warning(ErrorType::Other, "W1", "low disk");
    state.handle_error(warn);
    assert_eq!(
        state
            .error_manager
            .notifications()
            .iter_active()
            .last()
            .unwrap()
            .kind,
        NotificationType::Warning
    );

    // Info (if we had one, but new() defaults to error, warning defaults to warning)
    // RiftError doesn't have a factory for Info yet, but we can create it manually
    let info = RiftError {
        severity: ErrorSeverity::Info,
        kind: ErrorType::Other,
        code: "I1".to_string(),
        message: "info msg".to_string(),
    };
    state.handle_error(info);
    assert_eq!(
        state
            .error_manager
            .notifications()
            .iter_active()
            .last()
            .unwrap()
            .kind,
        NotificationType::Info
    );
}

#[test]
fn test_gutter_thresholds() {
    let mut state = State::new();
    // gutter_width: 2 (for "1 "), threshold: 10
    assert_eq!(state.gutter_width, 2);
    assert_eq!(state.next_gutter_threshold, 10);

    // Below threshold (9 lines)
    state.update_buffer_stats(9, 100, crate::document::LineEnding::LF);
    assert_eq!(state.gutter_width, 2);
    assert_eq!(state.next_gutter_threshold, 10);

    // Cross threshold (10 lines) -> width becomes 3 ("10 ")
    state.update_buffer_stats(10, 100, crate::document::LineEnding::LF);
    assert_eq!(state.gutter_width, 3);
    assert_eq!(state.next_gutter_threshold, 100);

    // Stay in bracket (99 lines)
    state.update_buffer_stats(99, 100, crate::document::LineEnding::LF);
    assert_eq!(state.gutter_width, 3);
    assert_eq!(state.next_gutter_threshold, 100);

    // Cross next threshold (100 lines) -> width becomes 4 ("100 ")
    state.update_buffer_stats(100, 100, crate::document::LineEnding::LF);
    assert_eq!(state.gutter_width, 4);
    assert_eq!(state.next_gutter_threshold, 1000);

    // Reversion logic: if < threshold / 10
    // At 100, threshold is 1000. 1000/10 = 100.
    // Drop to 99. 99 < 100 is true.
    // Should revert to width 3.
    state.update_buffer_stats(99, 100, crate::document::LineEnding::LF);
    assert_eq!(state.gutter_width, 3);
    assert_eq!(state.next_gutter_threshold, 100);

    // Massive drop
    state.update_buffer_stats(1, 100, crate::document::LineEnding::LF);
    assert_eq!(state.gutter_width, 2);
    assert_eq!(state.next_gutter_threshold, 10);
}
