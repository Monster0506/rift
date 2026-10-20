//! Tests for state management

use crate::key::Key;
use crate::state::{State, UserSettings};

#[test]
fn test_state_new() {
    let state = State::new();
    assert!(!state.debug_mode);
    assert_eq!(state.last_keypress, None);
    assert_eq!(state.cursor_pos, (0, 0));
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);
}

#[test]
fn test_state_default() {
    let state = State::default();
    assert!(!state.debug_mode);
    assert_eq!(state.last_keypress, None);
    assert_eq!(state.cursor_pos, (0, 0));
    assert_eq!(state.total_lines, 1);
    assert_eq!(state.buffer_size, 0);
}

#[test]
fn test_toggle_debug() {
    let mut state = State::new();
    assert!(!state.debug_mode);

    // Toggle on
    state.toggle_debug();
    assert!(state.debug_mode);

    // Toggle off
    state.toggle_debug();
    assert!(!state.debug_mode);

    // Toggle on again
    state.toggle_debug();
    assert!(state.debug_mode);
}

#[test]
fn test_update_keypress() {
    let mut state = State::new();
    assert_eq!(state.last_keypress, None);

    state.update_keypress(Key::char('a'));
    assert_eq!(state.last_keypress, Some(Key::char('a')));

    state.update_keypress(Key::char('b'));
    assert_eq!(state.last_keypress, Some(Key::char('b')));

    state.update_keypress(Key::UP);
    assert_eq!(state.last_keypress, Some(Key::UP));

    state.update_keypress(Key::ctrl('c'));
    assert_eq!(state.last_keypress, Some(Key::ctrl('c')));
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

    state.update_keypress(Key::char('h'));
    state.update_cursor(2, 5);
    state.update_buffer_stats(3, 100, crate::document::LineEnding::LF);
    assert!(!state.debug_mode);
    assert_eq!(state.last_keypress, Some(Key::char('h')));
    assert_eq!(state.cursor_pos, (2, 5));
    assert_eq!(state.total_lines, 3);
    assert_eq!(state.buffer_size, 100);

    state.toggle_debug();
    assert!(state.debug_mode);

    state.update_keypress(Key::DOWN);
    state.update_cursor(10, 20);
    state.update_buffer_stats(15, 200, crate::document::LineEnding::LF);

    assert!(state.debug_mode);
    assert_eq!(state.last_keypress, Some(Key::DOWN));
    assert_eq!(state.cursor_pos, (10, 20));
    assert_eq!(state.total_lines, 15);
    assert_eq!(state.buffer_size, 200);
}

#[test]
fn test_multiple_keypress_updates() {
    let mut state = State::new();

    // Simulate a sequence of keypresses
    let keys = vec![
        Key::char('i'),
        Key::char('n'),
        Key::char('s'),
        Key::char('e'),
        Key::char('r'),
        Key::char('t'),
        Key::ESC,
    ];

    for key in keys {
        state.update_keypress(key);
    }

    // Last keypress should be Escape
    assert_eq!(state.last_keypress, Some(Key::ESC));
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
    const RATIO_EPSILON: f64 = 1e-9;
    let settings = UserSettings::default();
    assert!(settings.editing.expand_tabs);
    assert_eq!(settings.tab_width, 4);
    assert_eq!(settings.default_border_chars, None);
    assert!((settings.command_line_window.width_ratio - 0.6).abs() < RATIO_EPSILON);
    assert_eq!(settings.command_line_window.min_width, 40);
    assert_eq!(settings.command_line_window.height, 3);
    assert!(settings.command_line_window.border);
    assert!(!settings.command_line_window.reverse_video);
}

#[test]
fn test_state_with_custom_settings() {
    let mut custom_settings = UserSettings::new();
    custom_settings.editing.expand_tabs = false;
    custom_settings.tab_width = 4;

    let state = State::with_settings(custom_settings);
    assert!(!state.settings.editing.expand_tabs);
    assert_eq!(state.settings.tab_width, 4);
    assert!(!state.debug_mode); // Runtime state should still be default
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

    state.handle_error(err);

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
fn test_command_line_cursor_tracks_multibyte_char_byte_length() {
    let mut state = State::new();
    state.append_to_command_line('a');
    state.append_to_command_line('é'); // 2 UTF-8 bytes
    state.append_to_command_line('b');

    assert_eq!(state.command_line, "aéb");
    assert_eq!(
        state.command_line_cursor,
        state.command_line.len(),
        "cursor must land on the byte length of the string, not the char count"
    );

    // Backspacing must remove the whole multibyte char without panicking on a
    // non-char-boundary byte index.
    state.remove_from_command_line();
    assert_eq!(state.command_line, "aé");
    state.remove_from_command_line();
    assert_eq!(state.command_line, "a");

    state.move_command_line_home();
    state.append_to_command_line('é');
    assert_eq!(state.command_line, "éa");
    state.move_command_line_right();
    assert_eq!(
        state.command_line_cursor, 3,
        "moving right over 'é' must advance by its 2-byte length, not 1"
    );
}

#[test]
fn test_command_line_word_motions_with_multibyte_chars() {
    let mut state = State::new();
    for ch in "é foo".chars() {
        state.append_to_command_line(ch);
    }
    assert_eq!(state.command_line_cursor, 6);

    state.move_command_line_word_left();
    assert_eq!(state.command_line_cursor, 3);
    state.move_command_line_word_left();
    assert_eq!(state.command_line_cursor, 0);

    state.move_command_line_word_right();
    assert_eq!(state.command_line_cursor, 3);
    state.move_command_line_word_right();
    assert_eq!(state.command_line_cursor, 6);

    state.delete_word_back_command_line();
    assert_eq!(state.command_line, "é ");
    assert_eq!(state.command_line_cursor, 3);
    state.delete_word_back_command_line();
    assert_eq!(state.command_line, "");
    assert_eq!(state.command_line_cursor, 0);
}

#[test]
fn test_gutter_width_follows_the_digit_count() {
    let mut state = State::new();
    assert_eq!(state.gutter_width, 3);

    for (lines, width) in [(9, 3), (10, 4), (99, 4), (100, 5), (99, 4), (1, 3)] {
        state.update_buffer_stats(lines, 100, crate::document::LineEnding::LF);
        assert_eq!(state.gutter_width, width, "{lines} lines");
    }
}

#[test]
fn test_gutter_cols_needs_the_document_and_the_global_setting() {
    let mut state = State::new();
    state.update_buffer_stats(120, 100, crate::document::LineEnding::LF);

    assert_eq!(state.gutter_cols(true), 5);
    assert_eq!(state.gutter_cols(false), 0);
    assert_eq!(state.doc_gutter_cols(true, 9), 3);
    assert_eq!(state.doc_gutter_cols(false, 9), 0);

    state.settings.show_line_numbers = false;
    assert_eq!(state.gutter_cols(true), 0);
    assert_eq!(state.doc_gutter_cols(true, 9), 0);
}
