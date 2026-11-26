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
    custom_settings.editing.ghost_cut = false;

    let state = State::with_settings(custom_settings);
    assert!(!state.settings.editing.ghost_cut);
    assert!(!state.debug_mode); // Runtime state should still be default
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

fn selection_style(settings: &UserSettings) -> Option<crate::layer::CellStyle> {
    settings
        .syntax_colors
        .as_ref()
        .and_then(|c| c.exact_style("ui.selection"))
}

#[test]
fn applying_a_theme_sets_its_name_and_colors() {
    use crate::color::theme::Theme;
    let mut settings = UserSettings::new();
    for theme in [
        Theme::dark(),
        Theme::gruvbox(),
        Theme::nordic(),
        Theme::light(),
    ] {
        settings.apply_theme(&theme);
        assert_eq!(settings.theme(), Some(theme.name()));
        assert_eq!(settings.editor_bg(), Some(theme.background));
        assert_eq!(settings.editor_fg(), Some(theme.foreground));
    }
}

#[test]
fn every_builtin_theme_gets_a_bg_only_selection_tint_from_its_editor_colors() {
    use crate::color::theme::Theme;
    use crate::color::Color;
    let expected = [
        (Theme::light(), (0xcc, 0xcc, 0xcc)),
        (Theme::dark(), (0x45, 0x45, 0x45)),
        (Theme::gruvbox(), (0x4f, 0x4c, 0x3d)),
        (Theme::nordic(), (0x4a, 0x51, 0x5e)),
    ];
    let mut settings = UserSettings::new();
    for (theme, (r, g, b)) in expected {
        settings.apply_theme(&theme);
        assert_eq!(
            selection_style(&settings),
            Some(crate::layer::CellStyle {
                bg: Some(Color::Rgb { r, g, b }),
                ..crate::layer::CellStyle::default()
            }),
            "theme {}",
            theme.name()
        );
    }
}

#[test]
fn switching_themes_clears_the_selection_options() {
    use crate::color::theme::Theme;
    use crate::color::Color;
    let mut settings = UserSettings::new();
    settings.selection_bg = Some(Color::Red);
    settings.selection_fg = Some(Color::White);
    settings.refresh_derived_highlights();

    settings.apply_theme(&Theme::nordic());

    assert_eq!(settings.selection_bg, None);
    assert_eq!(settings.selection_fg, None);
    assert_eq!(
        selection_style(&settings).map(|s| (s.fg, s.bg)),
        Some((
            None,
            Some(Color::Rgb {
                r: 0x4a,
                g: 0x51,
                b: 0x5e
            })
        ))
    );
}
