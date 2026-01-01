//! Tests for rendering system

use crate::buffer::TextBuffer;
use crate::key::Key;
use crate::layer::{Layer, LayerCompositor, LayerPriority};
use crate::mode::Mode;
use crate::render::{
    _format_key as format_key, calculate_cursor_column, render, CursorInfo, RenderCache,
    RenderContext, StatusDrawState,
};
use crate::state::State;
use crate::status::StatusBar;
use crate::test_utils::MockTerminal;
use crate::viewport::Viewport;

// ============================================================================
// Key formatting tests
// ============================================================================

#[test]
fn test_format_key_char() {
    assert_eq!(format_key(Key::Char('a')), "a");
    assert_eq!(format_key(Key::Char('Z')), "Z");
    assert_eq!(format_key(Key::Char(' ')), " ");
    assert_eq!(format_key(Key::Char('0')), "0");
}

#[test]
fn test_format_key_non_printable() {
    assert_eq!(format_key(Key::Char('\0')), "\\u{0000}");
    assert_eq!(format_key(Key::Char('\x1f')), "\\u{001f}");
    assert_eq!(format_key(Key::Char('\x7f')), "\\u{007f}");
}

#[test]
fn test_format_key_ctrl() {
    assert_eq!(format_key(Key::Ctrl(b'a')), "Ctrl+A");
    assert_eq!(format_key(Key::Ctrl(b'c')), "Ctrl+C");
    assert_eq!(format_key(Key::Ctrl(b'z')), "Ctrl+Z");
}

#[test]
fn test_format_key_arrows() {
    assert_eq!(format_key(Key::ArrowUp), "Up");
    assert_eq!(format_key(Key::ArrowDown), "Down");
    assert_eq!(format_key(Key::ArrowLeft), "Left");
    assert_eq!(format_key(Key::ArrowRight), "Right");
}

#[test]
fn test_format_key_special() {
    assert_eq!(format_key(Key::Backspace), "Backspace");
    assert_eq!(format_key(Key::Delete), "Delete");
    assert_eq!(format_key(Key::Enter), "Enter");
    assert_eq!(format_key(Key::Escape), "Esc");
    assert_eq!(format_key(Key::Tab), "Tab");
    assert_eq!(format_key(Key::Home), "Home");
    assert_eq!(format_key(Key::End), "End");
    assert_eq!(format_key(Key::PageUp), "PageUp");
    assert_eq!(format_key(Key::PageDown), "PageDown");
}

// ============================================================================
// Cursor column calculation tests
// ============================================================================

#[test]
fn test_calculate_cursor_column_single_line() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    // Cursor is at position 5 (after "hello")
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 5);
}

#[test]
fn test_calculate_cursor_column_multiline() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2\nline3").unwrap();
    // Move to start
    for _ in 0..18 {
        buf.move_left();
    }
    // Now cursor is at start of line 0
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 0);

    // Move to line 1
    buf.move_down();
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 0);

    // Move right 3 times on line 1
    buf.move_right();
    buf.move_right();
    buf.move_right();
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 3);
}

#[test]
fn test_calculate_cursor_column_empty_buffer() {
    let buf = TextBuffer::new(100).unwrap();
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 0);
}

#[test]
fn test_calculate_cursor_column_at_gap() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    // Move cursor to middle
    for _ in 0..3 {
        buf.move_left();
    }
    // Cursor should be at column 2
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 2);
}

#[test]
fn test_calculate_cursor_column_multiline_complex() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello\nworld\ntest").unwrap();

    // Move to start
    for _ in 0..17 {
        buf.move_left();
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 0);

    // Move to end of first line
    for _ in 0..5 {
        buf.move_right();
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 5);

    // Move to next line
    buf.move_right(); // Move past newline
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 0);

    // Move to middle of second line
    for _ in 0..3 {
        buf.move_right();
    }
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 3);
}

// ============================================================================
// Status bar layer rendering tests
// ============================================================================

#[test]
fn test_render_status_bar_normal_mode_layer() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 1, 80);
    let state = StatusDrawState {
        mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        file_name: "test.rs".to_string(),
        is_dirty: false,
        cursor: CursorInfo { row: 0, col: 0 },
        total_lines: 10,
        debug_mode: false,
        cols: 80,
        search_query: None,
        search_match_index: None,
        search_total_matches: 0,
        editor_bg: None,
        editor_fg: None,
        reverse_video: false,
    };

    StatusBar::render_to_layer(&mut layer, &state);

    // Check that "NORMAL" was written to the layer
    let cell = layer.get_cell(0, 0);
    assert!(cell.is_some());
}

#[test]
fn test_render_status_bar_insert_mode_layer() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 1, 80);
    let state = StatusDrawState {
        mode: Mode::Insert,
        pending_key: None,
        pending_count: 0,
        file_name: "test.rs".to_string(),
        is_dirty: false,
        cursor: CursorInfo { row: 0, col: 0 },
        total_lines: 10,
        debug_mode: false,
        cols: 80,
        search_query: None,
        search_match_index: None,
        search_total_matches: 0,
        editor_bg: None,
        editor_fg: None,
        reverse_video: false,
    };

    StatusBar::render_to_layer(&mut layer, &state);

    let cell = layer.get_cell(0, 0);
    assert!(cell.is_some());
}

#[test]
fn test_render_status_bar_pending_key_layer() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 1, 80);
    let state = StatusDrawState {
        mode: Mode::Normal,
        pending_key: Some(Key::Char('d')),
        pending_count: 0,
        file_name: "test.rs".to_string(),
        is_dirty: false,
        cursor: CursorInfo { row: 0, col: 0 },
        total_lines: 10,
        debug_mode: false,
        cols: 80,
        search_query: None,
        search_match_index: None,
        search_total_matches: 0,
        editor_bg: None,
        editor_fg: None,
        reverse_video: false,
    };

    StatusBar::render_to_layer(&mut layer, &state);

    let cell = layer.get_cell(0, 0);
    assert!(cell.is_some());
}

// ============================================================================
// Full render tests with compositor
// ============================================================================

#[test]
fn test_render_clears_screen() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(100).unwrap();
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    let mut compositor = LayerCompositor::new(10, 80);

    render(
        &mut term,
        &mut compositor,
        RenderContext {
            buf: &buf,
            viewport: &viewport,
            current_mode: Mode::Normal,
            pending_key: None,
            pending_count: 0,
            state: &state,
            needs_clear: true,
            tab_width: 4,
            highlights: None,
        },
        &mut RenderCache::default(),
    )
    .unwrap();

    // First render should clear screen
    assert!(term.clear_screen_calls >= 1);
}

#[test]
fn test_render_cursor_positioning() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    let mut compositor = LayerCompositor::new(10, 80);

    render(
        &mut term,
        &mut compositor,
        RenderContext {
            buf: &buf,
            viewport: &viewport,
            current_mode: Mode::Normal,
            pending_key: None,
            pending_count: 0,
            state: &state,
            needs_clear: true,
            tab_width: 4,
            highlights: None,
        },
        &mut RenderCache::default(),
    )
    .unwrap();

    // Should have moved cursor
    assert!(!term.cursor_moves.is_empty());
}

#[test]
fn test_render_empty_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(100).unwrap();
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    let mut compositor = LayerCompositor::new(10, 80);

    render(
        &mut term,
        &mut compositor,
        RenderContext {
            buf: &buf,
            viewport: &viewport,
            current_mode: Mode::Normal,
            pending_key: None,
            pending_count: 0,
            state: &state,
            needs_clear: true,
            tab_width: 4,
            highlights: None,
        },
        &mut RenderCache::default(),
    )
    .unwrap();

    // First render should clear screen
    assert!(term.clear_screen_calls >= 1);
    // Should still render empty lines
    assert!(!term.writes.is_empty());
}

#[test]
fn test_render_multiline_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2\nline3\nline4\nline5").unwrap();
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    let mut compositor = LayerCompositor::new(10, 80);

    render(
        &mut term,
        &mut compositor,
        RenderContext {
            buf: &buf,
            viewport: &viewport,
            current_mode: Mode::Normal,
            pending_key: None,
            pending_count: 0,
            state: &state,
            needs_clear: true,
            tab_width: 4,
            highlights: None,
        },
        &mut RenderCache::default(),
    )
    .unwrap();

    let written = term.get_written_string();
    assert!(written.contains("line1"));
    assert!(written.contains("line5"));
}
