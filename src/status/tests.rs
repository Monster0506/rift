//! Tests for status bar

use crate::key::Key;
use crate::mode::Mode;
use crate::state::State;
use crate::status::StatusBar;
use crate::test_utils::MockTerminal;
use crate::viewport::Viewport;

#[test]
fn test_format_mode() {
    assert_eq!(StatusBar::format_mode(Mode::Normal), "NORMAL");
    assert_eq!(StatusBar::format_mode(Mode::Insert), "INSERT");
    assert_eq!(StatusBar::format_mode(Mode::Command), ":");
}

#[test]
fn test_status_bar_render_command_mode() {
    use crate::state::State;
    use crate::test_utils::MockTerminal;
    use crate::viewport::Viewport;

    let mut term = MockTerminal::new(24, 80);
    let viewport = Viewport::new(24, 80);
    let state = State::new();

    StatusBar::render(&mut term, &viewport, Mode::Command, None, &state).unwrap();

    let written = term.get_written_string();
    // Command mode should show colon prompt
    assert!(written.contains(":"));
    // Should not show NORMAL or INSERT
    assert!(!written.contains("NORMAL"));
    assert!(!written.contains("INSERT"));
}

#[test]
fn test_format_key_char() {
    assert_eq!(StatusBar::format_key(Key::Char(b'a')), "a");
    assert_eq!(StatusBar::format_key(Key::Char(b'Z')), "Z");
    assert_eq!(StatusBar::format_key(Key::Char(b' ')), " ");
}

#[test]
fn test_format_key_ctrl() {
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'a')), "Ctrl+A");
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'c')), "Ctrl+C");
}

#[test]
fn test_format_key_arrows() {
    assert_eq!(StatusBar::format_key(Key::ArrowUp), "↑");
    assert_eq!(StatusBar::format_key(Key::ArrowDown), "↓");
    assert_eq!(StatusBar::format_key(Key::ArrowLeft), "←");
    assert_eq!(StatusBar::format_key(Key::ArrowRight), "→");
}

#[test]
fn test_format_key_special() {
    assert_eq!(StatusBar::format_key(Key::Backspace), "Backspace");
    assert_eq!(StatusBar::format_key(Key::Delete), "Delete");
    assert_eq!(StatusBar::format_key(Key::Enter), "Enter");
    assert_eq!(StatusBar::format_key(Key::Escape), "Esc");
}

#[test]
fn test_status_bar_render_normal_mode() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    let written = term.get_written_string();
    assert!(written.contains("NORMAL"));
    assert!(!written.contains("INSERT"));
}

#[test]
fn test_status_bar_render_insert_mode() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();

    StatusBar::render(&mut term, &viewport, Mode::Insert, None, &state).unwrap();

    let written = term.get_written_string();
    assert!(written.contains("INSERT"));
    assert!(!written.contains("NORMAL"));
}

#[test]
fn test_status_bar_render_pending_key() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();

    StatusBar::render(
        &mut term,
        &viewport,
        Mode::Normal,
        Some(Key::Char(b'd')),
        &state,
    )
    .unwrap();

    let written = term.get_written_string();
    assert!(written.contains("[d]"));
}

#[test]
fn test_status_bar_render_debug_mode() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.toggle_debug();
    state.update_keypress(Key::Char(b'a'));
    state.update_cursor(5, 10);
    state.update_buffer_stats(10, 100);

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    let written = term.get_written_string();
    assert!(written.contains("Last: a"));
    assert!(written.contains("Pos: 6:11")); // 1-indexed
    assert!(written.contains("Lines: 10"));
    assert!(written.contains("Size: 100B"));
}

#[test]
fn test_status_bar_render_debug_with_pending() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.toggle_debug();
    state.update_keypress(Key::ArrowUp);

    StatusBar::render(
        &mut term,
        &viewport,
        Mode::Normal,
        Some(Key::Char(b'd')),
        &state,
    )
    .unwrap();

    let written = term.get_written_string();
    assert!(written.contains("NORMAL"));
    assert!(written.contains("[d]"));
    assert!(written.contains("Last: ↑"));
}

#[test]
fn test_status_bar_render_fills_line() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    // Status bar should fill the entire line width
    let total_written: usize = term.writes.iter().map(|w| w.len()).sum();
    // Should write at least the viewport width
    assert!(total_written >= 80);
}

#[test]
fn test_status_bar_render_reverse_video() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.settings.status_line.reverse_video = true;

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    let written = term.get_written_bytes();
    // Should contain reverse video escape sequence
    assert!(written.contains(&b'\x1b'));
    let written_str = term.get_written_string();
    assert!(written_str.contains("\x1b[7m")); // Reverse video
    assert!(written_str.contains("\x1b[0m")); // Reset
}

#[test]
fn test_status_bar_render_reverse_video_off() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    let written = term.get_written_bytes();
    // Should contain reverse video escape sequence
    assert!(!written.contains(&b'\x1b'));
    let written_str = term.get_written_string();
    assert!(!written_str.contains("\x1b[7m")); // Reverse video
    assert!(!written_str.contains("\x1b[0m")); // Reset
}

#[test]
fn test_status_bar_debug_truncation() {
    let mut term = MockTerminal::new(10, 20); // Narrow viewport
    let viewport = Viewport::new(10, 20);
    let mut state = State::new();
    state.toggle_debug();
    state.update_keypress(Key::Char(b'a'));
    state.update_cursor(100, 200);
    state.update_buffer_stats(1000, 50000);

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    // Debug info should be truncated if too long
    let written = term.get_written_string();
    // Should still contain mode
    assert!(written.contains("NORMAL"));
}

#[test]
fn test_status_bar_various_keys() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();

    // Test various pending keys
    let keys = vec![Key::Char(b'a'), Key::ArrowUp, Key::Ctrl(b'c'), Key::Escape];

    for key in keys {
        term.writes.clear();
        StatusBar::render(&mut term, &viewport, Mode::Normal, Some(key), &state).unwrap();
        let written = term.get_written_string();
        assert!(written.contains("["));
        assert!(written.contains("]"));
    }
}

#[test]
fn test_status_bar_filename_shown_when_enabled() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.update_filename("test.txt".to_string());
    // show_filename defaults to true

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    let written = term.get_written_string();
    assert!(written.contains("test.txt"));
}

#[test]
fn test_status_bar_filename_hidden_when_disabled() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.update_filename("test.txt".to_string());
    state.settings.status_line.show_filename = false;

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    let written = term.get_written_string();
    assert!(!written.contains("test.txt"));
}

#[test]
fn test_status_bar_filename_always_shown_in_debug() {
    let mut term = MockTerminal::new(10, 120);
    let viewport = Viewport::new(10, 120);
    let mut state = State::new();
    state.set_file_path(Some("c:\\Users\\test\\file.txt".to_string()));
    state.toggle_debug(); // Enable debug mode
    state.settings.status_line.show_filename = false; // Disable filename setting

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    let written = term.get_written_string();
    // In debug mode, filepath should appear even when show_filename is false
    assert!(written.contains("File:"));
    assert!(written.contains("file.txt"));
}

#[test]
fn test_status_bar_no_name_display() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    // file_name defaults to "[No Name]"

    StatusBar::render(&mut term, &viewport, Mode::Normal, None, &state).unwrap();

    let written = term.get_written_string();
    assert!(written.contains("[No Name]"));
}
