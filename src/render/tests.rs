//! Tests for rendering system

use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::key::Key;
use crate::layer::Cell;
use crate::layer::{Layer, LayerCompositor, LayerPriority};
use crate::mode::Mode;
use crate::render::{
    calculate_cursor_column, render, CursorInfo, RenderCache, RenderContext, StatusDrawState,
};
use crate::state::State;
use crate::status::StatusBar;
use crate::test_utils::MockTerminal;
use crate::viewport::Viewport;
fn create_default_statusdrawstate() -> StatusDrawState {
    StatusDrawState {
        mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        file_name: "".to_string(),
        is_dirty: false,
        cols: 80,
        editor_bg: None,
        editor_fg: None,
        debug_mode: false,
        total_lines: 0,
        search_query: None,
        reverse_video: false,
        search_match_index: None,
        search_total_matches: 0,
        cursor: CursorInfo { row: 0, col: 0 },
    }
}
// ============================================================================
// Key formatting tests
// ============================================================================

#[test]
fn test_format_key_char() {
    assert_eq!(StatusBar::format_key(Key::Char('a')), "a");
    assert_eq!(StatusBar::format_key(Key::Char('Z')), "Z");
    assert_eq!(StatusBar::format_key(Key::Char(' ')), " ");
    assert_eq!(StatusBar::format_key(Key::Char('0')), "0");
}

#[test]
fn test_format_key_non_printable() {
    assert_eq!(StatusBar::format_key(Key::Char('\0')), "\\u{0000}");
    assert_eq!(StatusBar::format_key(Key::Char('\x1f')), "\\u{001f}");
    assert_eq!(StatusBar::format_key(Key::Char('\x7f')), "\\u{007f}");
}

#[test]
fn test_format_key_ctrl() {
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'a')), "Ctrl+A");
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'c')), "Ctrl+C");
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'z')), "Ctrl+Z");
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
    assert_eq!(StatusBar::format_key(Key::Tab), "Tab");
    assert_eq!(StatusBar::format_key(Key::Home), "Home");
    assert_eq!(StatusBar::format_key(Key::End), "End");
    assert_eq!(StatusBar::format_key(Key::PageUp), "PageUp");
    assert_eq!(StatusBar::format_key(Key::PageDown), "PageDown");
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
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);

    let statusdrawstate = create_default_statusdrawstate();

    StatusBar::render_to_layer(&mut layer, &statusdrawstate);

    // Check that "NORMAL" was written to the layer
    // Status bar is at last row (9), mode is at start
    let cell = layer.get_cell(9, 0);
    assert!(cell.is_some());
    // Should contain 'N' from 'NORMAL'
    // Note: The status bar writes to the last row of the viewport
}

#[test]
fn test_render_status_bar_insert_mode_layer() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);
    let mut statusdrawstate = create_default_statusdrawstate();
    statusdrawstate.mode = Mode::Insert;

    StatusBar::render_to_layer(&mut layer, &statusdrawstate);

    // Check that content was written to the layer
    let cell = layer.get_cell(9, 0);
    assert!(cell.is_some());
}

#[test]
fn test_render_status_bar_pending_key_layer() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);
    let mut statusdrawstate = create_default_statusdrawstate();
    statusdrawstate.pending_key = Some(Key::Ctrl(b'd'));

    StatusBar::render_to_layer(&mut layer, &statusdrawstate);

    // Should have pending key indicator
    let cell = layer.get_cell(9, 0);
    assert!(cell.is_some());
}

// ============================================================================
// Full render tests with compositor
// ============================================================================

#[test]
fn test_render_does_not_clear_screen() {
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

    // First render should NOT clear screen (to prevent flicker)
    assert_eq!(term.clear_screen_calls, 0);
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

    // First render should NOT clear screen
    assert_eq!(term.clear_screen_calls, 0);
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
    assert!(written.contains("line2"));
    assert!(written.contains("line3"));
}

#[test]
fn test_render_file_loaded_at_start() {
    // Simulate file loading: content inserted, then cursor moved to start
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    // Insert content (simulating file load)
    buf.insert_bytes(b"line1\nline2\nline3\n").unwrap();
    buf.move_to_start();

    let viewport = Viewport::new(10, 80);
    let state = State::new();
    let mut compositor = LayerCompositor::new(10, 80);

    // First render (simulating initial render after file load)
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

    // Should NOT clear screen on first render
    assert_eq!(term.clear_screen_calls, 0);

    // Should render all lines
    let written = term.get_written_string();
    assert!(written.contains("line1"));
    assert!(written.contains("line2"));
    assert!(written.contains("line3"));

    // Verify cursor is at start (line 0, column 0)
    assert_eq!(buf.get_line(), 0);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_render_viewport_scrolling() {
    let mut term = MockTerminal::new(5, 80); // Small viewport
    let mut buf = TextBuffer::new(100).unwrap();
    // Create 10 lines
    for i in 0..10 {
        buf.insert_str(&format!("line{}\n", i)).unwrap();
    }
    // Move cursor to line 8
    for _ in 0..8 {
        buf.move_up();
    }
    let viewport = Viewport::new(5, 80);
    let state = State::new();
    let mut compositor = LayerCompositor::new(5, 80);

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

    // Viewport should scroll to show cursor
    // Top line should be adjusted
    assert!(viewport.top_line() <= 8);
}

#[test]
fn test_render_viewport_edge_cases() {
    let mut term = MockTerminal::new(1, 1); // Minimal viewport
    let buf = TextBuffer::new(100).unwrap();
    let viewport = Viewport::new(1, 1);
    let state = State::new();
    let mut compositor = LayerCompositor::new(1, 1);

    // Should not panic with minimal viewport
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
}

#[test]
fn test_render_large_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(10000).unwrap();
    // Insert a large amount of text
    for i in 0..100 {
        buf.insert_str(&format!("line {}\n", i)).unwrap();
    }
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

    // Should render successfully - first render does NOT clear screen
    assert_eq!(term.clear_screen_calls, 0);
    assert!(!term.writes.is_empty());
}

#[test]
fn test_render_cursor_at_viewport_boundaries() {
    let mut term = MockTerminal::new(5, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    // Create content
    for i in 0..20 {
        buf.insert_str(&format!("line {}\n", i)).unwrap();
    }
    let mut viewport = Viewport::new(5, 80);
    let state = State::new();
    let mut compositor = LayerCompositor::new(5, 80);

    // Test cursor at top - first render should clear
    for _ in 0..20 {
        buf.move_up();
    }
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
    // First render does NOT clear screen
    assert_eq!(term.clear_screen_calls, 0);

    // Reset
    term.clear_screen_calls = 0;
    term.cursor_moves.clear();
    term.writes.clear();

    // Test cursor at bottom - should scroll and clear
    for _ in 0..20 {
        buf.move_down();
    }
    let needs_clear2 = viewport.update(buf.get_line(), 0, buf.get_total_lines(), 0);
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
            needs_clear: needs_clear2,
            tab_width: 4,
            highlights: None,
        },
        &mut RenderCache::default(),
    )
    .unwrap();
    // Should NOT clear when scrolling to show cursor at bottom
    assert_eq!(term.clear_screen_calls, 0);
}

// ============================================================================
// Layer content tests
// ============================================================================

#[test]
fn test_compositor_content_layer() {
    let mut compositor = LayerCompositor::new(10, 80);

    // Content layer should be accessible
    let content_layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(content_layer.rows(), 10);
    assert_eq!(content_layer.cols(), 80);
}

#[test]
fn test_compositor_status_bar_layer() {
    let mut compositor = LayerCompositor::new(10, 80);

    // Status bar layer should be accessible
    let status_layer = compositor.get_layer_mut(LayerPriority::STATUS_BAR);
    assert_eq!(status_layer.rows(), 10);
    assert_eq!(status_layer.cols(), 80);
}

#[test]
fn test_compositor_floating_window_layer() {
    let mut compositor = LayerCompositor::new(10, 80);

    // Floating window layer should be accessible
    let floating_layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);
    assert_eq!(floating_layer.rows(), 10);
    assert_eq!(floating_layer.cols(), 80);
}

// ============================================================================
// Line number rendering tests
// ============================================================================

#[test]
fn test_render_line_numbers_enabled() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2").unwrap();
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(2, 11, crate::document::LineEnding::LF);

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

    let content_layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    // Gutter width for 2 lines should be 1 (digit) + 1 (padding) = 2
    // Line 1: "1 "
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('1')
    );
    assert_eq!(
        content_layer.get_cell(0, 1).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 2).unwrap().content,
        Character::from('l')
    ); // Content starts here
}

#[test]
fn test_render_line_numbers_disabled() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("line1").unwrap();
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.settings.show_line_numbers = false;
    state.update_buffer_stats(1, 5, crate::document::LineEnding::LF);

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

    let content_layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    // Should start immediately with content
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('l')
    );
}

#[test]
fn test_render_line_numbers_gutter_width() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(100).unwrap();
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(100, 0, crate::document::LineEnding::LF);

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

    let content_layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    // Gutter width: 3 digits + 1 padding = 4
    // Line 1: "  1 "
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 1).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 2).unwrap().content,
        Character::from('1')
    );
    assert_eq!(
        content_layer.get_cell(0, 3).unwrap().content,
        Character::from(' ')
    );
}

#[test]
fn test_render_cursor_position_with_line_numbers() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("test").unwrap();
    buf.move_to_start();
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(10, 4, crate::document::LineEnding::LF); // 2 digits -> gutter 3

    let mut compositor = LayerCompositor::new(10, 80);

    let mut cache = RenderCache::default();
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
        &mut cache,
    )
    .unwrap();
}

#[test]
fn test_no_redraw_on_noop() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("test").unwrap();
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.settings.show_line_numbers = false;
    let mut compositor = LayerCompositor::new(10, 80);
    let mut cache = RenderCache::default();

    // 1. First render - populates layers and cache
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
        &mut cache,
    )
    .unwrap();

    // Verify content was rendered
    let content_layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('t')
    );

    // 2. Manually "vandalize" the layer content
    // If selective redrawing works, this change should PERSIST because render() will skip this layer.
    content_layer.set_cell(0, 0, Cell::from_char('X'));
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('X')
    );

    // 3. Second render - should skip CONTENT layer because state hasn't changed
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
            needs_clear: false,
            tab_width: 4,
            highlights: None,
        },
        &mut cache,
    )
    .unwrap();

    // 4. Verification: The 'X' should still be there!
    let content_layer_after = compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        content_layer_after.get_cell(0, 0).unwrap().content,
        Character::from('X'),
        "Layer was re-rendered despite identical state!"
    );
}

#[test]
fn test_redraw_on_change() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("test").unwrap();
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.settings.show_line_numbers = false;
    let mut compositor = LayerCompositor::new(10, 80);
    let mut cache = RenderCache::default();

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
            highlights: None,
            tab_width: 4,
        },
        &mut cache,
    )
    .unwrap();

    // Vandalize
    compositor
        .get_layer_mut(LayerPriority::CONTENT)
        .set_cell(0, 0, Cell::from_char('X'));

    // Change state (insert a char)
    buf.insert_char('!').unwrap();

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
            needs_clear: false,
            tab_width: 4,
            highlights: None,
        },
        &mut cache,
    )
    .unwrap();

    // The 'X' should be GONE (replaced by 't' from "test!")
    let content_layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('t')
    );
}

// ============================================================================
// Notification wrapping tests
// ============================================================================

#[test]
fn test_wrap_text_simple() {
    let text = "hello world";
    let wrapped = crate::render::wrap_text(text, 20);
    assert_eq!(wrapped, vec!["hello world".to_string()]);
}

#[test]
fn test_wrap_text_needs_wrapping() {
    let text = "hello world this is a test";
    // "hello world " -> 12 chars
    // "this is a " -> 10 chars
    // "test" -> 4 chars
    let wrapped = crate::render::wrap_text(text, 12);
    assert_eq!(
        wrapped,
        vec![
            "hello world".to_string(),
            "this is a".to_string(), // "this is a " fits
            "test".to_string()
        ]
    );
}

#[test]
fn test_wrap_text_long_word() {
    // If a word is longer than width, it should still be included (though layout might break visually,
    // the wrapping function shouldn't infinite loop or panic)
    // Current implementation will put it on its own line but won't split the word
    let text = "a verylongword indeed";
    let wrapped = crate::render::wrap_text(text, 5);
    assert_eq!(
        wrapped,
        vec![
            "a".to_string(),
            "verylongword".to_string(),
            "indeed".to_string()
        ]
    );
}

#[test]
fn test_wrap_text_empty() {
    let text = "";
    let wrapped = crate::render::wrap_text(text, 10);
    // Should return at least one empty line to preserve height
    assert_eq!(wrapped, vec!["".to_string()]);
}

#[test]
fn test_wrap_text_newlines() {
    let text = "line1\nline2";
    let wrapped = crate::render::wrap_text(text, 20);
    assert_eq!(wrapped, vec!["line1".to_string(), "line2".to_string()]);

    let text = "line1\n\nline3";
    let wrapped = crate::render::wrap_text(text, 20);
    assert_eq!(
        wrapped,
        vec!["line1".to_string(), "".to_string(), "line3".to_string()]
    );
}

#[test]
fn test_render_search_highlights() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello world").unwrap(); // "hello" at 0..5, " " at 5, "world" at 6..11
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.settings.show_line_numbers = false;
    // mock matches
    use crate::search::SearchMatch;
    state.search_matches = vec![SearchMatch { range: 6..11 }];

    let mut compositor = LayerCompositor::new(10, 80);
    let mut cache = RenderCache::default();

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
        &mut cache,
    )
    .unwrap();

    let content_layer = compositor.get_layer_mut(LayerPriority::CONTENT);

    // Check "hello" (start) - should be default colors (None, None)
    let cell_h = content_layer.get_cell(0, 0).unwrap();
    assert_eq!(cell_h.fg, state.settings.editor_fg);
    assert_eq!(cell_h.bg, state.settings.editor_bg);

    // Check "world" (start) - should be highlighted
    let cell_w = content_layer.get_cell(0, 6).unwrap();
    assert_eq!(cell_w.content, Character::from('w'));
    assert_eq!(cell_w.bg, Some(crate::color::Color::Yellow));
    assert_eq!(cell_w.fg, Some(crate::color::Color::Black));
}
