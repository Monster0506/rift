//! Tests for status bar

use crate::key::Key;
use crate::layer::{Layer, LayerPriority};
use crate::mode::Mode;
use crate::render::{CursorInfo, StatusDrawState};
use crate::status::StatusBar;

fn default_state() -> StatusDrawState {
    StatusDrawState {
        mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        last_keypress: None,
        file_name: "[No Name]".to_string(),
        is_dirty: false,
        cursor: CursorInfo { row: 0, col: 0 },
        total_lines: 0,
        debug_mode: false,
        cols: 80,
        search_query: None,
        search_match_index: None,
        search_total_matches: 0,
        reverse_video: false,
        show_status_line: true,
        show_filename: true,
        show_dirty_indicator: true,
        editor_bg: None,
        editor_fg: None,
        lsp_status: None,
        lsp_ok_color: None,
        lsp_error_color: None,
        lsp_warn_color: None,
        is_remote: false,
    }
}

/// Render `state` into a fresh `rows`x`cols` layer and read the status row
/// back as plain text (blank cells become spaces).
fn render_row_text(state: &StatusDrawState, rows: usize, cols: usize) -> String {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, rows, cols);
    StatusBar::render_to_layer(&mut layer, state);
    let row = rows.saturating_sub(1);
    (0..cols)
        .map(|col| {
            layer
                .get_cell(row, col)
                .map(|c| c.content.to_char_lossy())
                .unwrap_or(' ')
        })
        .collect()
}

#[test]
fn test_format_mode() {
    assert_eq!(StatusBar::format_mode(Mode::Normal), "NORMAL");
    assert_eq!(StatusBar::format_mode(Mode::Insert), "INSERT");
    assert_eq!(StatusBar::format_mode(Mode::Command), "COMMAND");
}

#[test]
fn test_format_key_char() {
    assert_eq!(StatusBar::format_key(Key::Char('a')), "a");
    assert_eq!(StatusBar::format_key(Key::Char('Z')), "Z");
    assert_eq!(StatusBar::format_key(Key::Char(' ')), " ");
}

#[test]
fn test_format_key_ctrl() {
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'a')), "Ctrl+A");
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'c')), "Ctrl+C");
}

#[test]
fn test_format_key_arrows() {
    assert_eq!(StatusBar::format_key(Key::ArrowUp), "Up");
    assert_eq!(StatusBar::format_key(Key::ArrowDown), "Down");
    assert_eq!(StatusBar::format_key(Key::ArrowLeft), "Left");
    assert_eq!(StatusBar::format_key(Key::ArrowRight), "Right");
}

#[test]
fn test_format_key_special() {
    assert_eq!(StatusBar::format_key(Key::Backspace), "Backspace");
    assert_eq!(StatusBar::format_key(Key::Delete), "Delete");
    assert_eq!(StatusBar::format_key(Key::Enter), "Enter");
    assert_eq!(StatusBar::format_key(Key::Escape), "Esc");
}

#[test]
fn test_status_bar_render_command_mode() {
    let mut state = default_state();
    state.mode = Mode::Command;

    let written = render_row_text(&state, 24, 80);
    assert!(written.contains("COMMAND"));
    assert!(!written.contains("NORMAL"));
    assert!(!written.contains("INSERT"));
}

#[test]
fn test_status_bar_filename_truncation_does_not_panic_on_multibyte() {
    let mut state = default_state();
    state.file_name = "résumé_набор_文件名".to_string();
    render_row_text(&state, 10, 20);
}

#[test]
fn test_status_bar_debug_truncation_does_not_panic_on_multibyte() {
    let mut state = default_state();
    state.debug_mode = true;
    state.file_name = "résumé_набор_文件名".to_string();
    render_row_text(&state, 10, 20);
}

#[test]
fn test_status_bar_render_debug_mode() {
    let mut state = default_state();
    state.debug_mode = true;
    state.last_keypress = Some(Key::Char('a'));
    state.cursor = CursorInfo { row: 5, col: 10 };
    state.total_lines = 10;

    let written = render_row_text(&state, 10, 80);
    assert!(written.contains("Last: a"));
    assert!(written.contains("Pos: 6:11")); // 1-indexed
    assert!(written.contains("Lines: 10"));
}

#[test]
fn test_status_bar_render_debug_with_pending() {
    let mut state = default_state();
    state.debug_mode = true;
    state.last_keypress = Some(Key::ArrowUp);
    state.pending_key = Some(Key::Char('d'));

    let written = render_row_text(&state, 10, 80);
    assert!(written.contains("NORMAL"));
    assert!(written.contains("[d]"));
    assert!(written.contains("Last: Up"));
}

#[test]
fn test_status_bar_render_fills_line() {
    let state = default_state();
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);
    StatusBar::render_to_layer(&mut layer, &state);

    // The status row should be painted edge to edge, including the last column.
    assert!(layer.get_cell(9, 0).is_some());
    assert!(layer.get_cell(9, 79).is_some());
}

#[test]
fn test_status_bar_render_reverse_video() {
    let mut state = default_state();
    state.reverse_video = true;

    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);
    StatusBar::render_to_layer(&mut layer, &state);

    let cell = layer.get_cell(9, 0).unwrap();
    // Reverse video swaps in Black-on-White when no theme colors are set.
    assert_eq!(cell.fg, Some(crate::color::Color::Black));
    assert_eq!(cell.bg, Some(crate::color::Color::White));
}

#[test]
fn test_status_bar_render_reverse_video_off() {
    let state = default_state();

    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);
    StatusBar::render_to_layer(&mut layer, &state);

    let cell = layer.get_cell(9, 0).unwrap();
    assert_eq!(cell.fg, None);
    assert_eq!(cell.bg, None);
}

#[test]
fn test_status_bar_debug_truncation() {
    let mut state = default_state();
    state.cols = 20; // Narrow viewport
    state.debug_mode = true;
    state.last_keypress = Some(Key::Char('a'));
    state.cursor = CursorInfo { row: 100, col: 200 };
    state.total_lines = 1000;

    let written = render_row_text(&state, 10, 20);
    // Debug info should be truncated if too long; mode should still show.
    assert!(written.contains("NORMAL"));
}

#[test]
fn test_status_bar_various_keys() {
    let keys = vec![Key::Char('a'), Key::ArrowUp, Key::Ctrl(b'c'), Key::Escape];

    for key in keys {
        let mut state = default_state();
        state.pending_key = Some(key);
        let written = render_row_text(&state, 10, 80);
        assert!(written.contains('['));
        assert!(written.contains(']'));
    }
}

#[test]
fn test_status_bar_filename_shown_when_enabled() {
    let mut state = default_state();
    state.file_name = "test.txt".to_string();
    // show_filename defaults to true

    let written = render_row_text(&state, 10, 80);
    assert!(written.contains("test.txt"));
}

#[test]
fn test_status_bar_filename_hidden_when_disabled() {
    let mut state = default_state();
    state.file_name = "test.txt".to_string();
    state.show_filename = false;

    let written = render_row_text(&state, 10, 80);
    assert!(!written.contains("test.txt"));
}

#[test]
fn test_status_bar_filename_always_shown_in_debug() {
    let mut state = default_state();
    state.file_name = "file.txt".to_string();
    state.debug_mode = true;
    state.show_filename = false; // Disabled, but debug mode ignores it

    let written = render_row_text(&state, 10, 120);
    assert!(written.contains("File:"));
    assert!(written.contains("file.txt"));
}

#[test]
fn test_status_bar_no_name_display() {
    let state = default_state();
    // file_name defaults to "[No Name]"

    let written = render_row_text(&state, 10, 80);
    assert!(written.contains("[No Name]"));
}

#[test]
fn test_status_bar_dirty_indicator() {
    let mut state = default_state();
    state.file_name = "test.txt".to_string();
    state.is_dirty = true;
    // show_dirty_indicator defaults to true

    let written = render_row_text(&state, 10, 80);
    assert!(written.contains("test.txt*"));
}

#[test]
fn test_status_bar_dirty_indicator_hidden_when_disabled() {
    let mut state = default_state();
    state.file_name = "test.txt".to_string();
    state.is_dirty = true;
    state.show_dirty_indicator = false;

    let written = render_row_text(&state, 10, 80);
    assert!(written.contains("test.txt"));
    assert!(!written.contains("test.txt*"));
}

#[test]
fn test_status_bar_hidden_when_show_status_line_disabled() {
    let mut state = default_state();
    state.file_name = "test.txt".to_string();
    state.show_status_line = false;

    let written = render_row_text(&state, 10, 80);
    assert!(!written.contains("NORMAL"));
    assert!(!written.contains("test.txt"));
    assert!(written.trim().is_empty());
}
