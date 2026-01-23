//! Tests for rendering system

use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::key::Key;
use crate::layer::Cell;
use crate::layer::{Layer, LayerPriority};
use crate::mode::Mode;
use crate::render::{
    calculate_cursor_column, CursorInfo, RenderState, RenderSystem, StatusDrawState,
};
use crate::state::State;
use crate::status::StatusBar;
use crate::test_utils::MockTerminal;

fn create_default_statusdrawstate() -> StatusDrawState {
    StatusDrawState {
        mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        last_keypress: None,
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
    let state = State::new();
    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
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
    let state = State::new();
    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();

    // Should have moved cursor
    assert!(!term.cursor_moves.is_empty());
}

#[test]
fn test_render_empty_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(100).unwrap();
    let state = State::new();
    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
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
    let state = State::new();
    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
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

    let state = State::new();
    let mut system = RenderSystem::new(10, 80);

    // First render (simulating initial render after file load)
    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
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
    let state = State::new();
    let mut system = RenderSystem::new(5, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();

    // Viewport should scroll to show cursor
    // Top line should be adjusted
    assert!(system.viewport.top_line() <= 8);
}

#[test]
fn test_render_viewport_edge_cases() {
    let mut term = MockTerminal::new(1, 1); // Minimal viewport
    let buf = TextBuffer::new(100).unwrap();
    let state = State::new();
    let mut system = RenderSystem::new(1, 1);

    // Should not panic with minimal viewport
    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
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
    let state = State::new();
    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
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
    let state = State::new();
    let mut system = RenderSystem::new(5, 80);

    // Test cursor at top - first render should clear
    for _ in 0..20 {
        buf.move_up();
    }
    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
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
    // Manually update viewport as Editor would
    system
        .viewport
        .update(buf.get_line(), 0, buf.get_total_lines(), 0);
    // Note: RenderSystem manages updates automatically, but we can verify scrolling happens

    // We want to simulate a second frame where changed cells trigger clear?
    // Or just check that scrolling happened

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: false,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();
    // Should NOT clear when scrolling to show cursor at bottom (renderer logic attempts to minimize clears)
    assert_eq!(term.clear_screen_calls, 0);
    assert!(system.viewport.top_line() > 0);
}

// ============================================================================
// Layer content tests
// ============================================================================

#[test]
fn test_compositor_content_layer() {
    let mut system = RenderSystem::new(10, 80);

    // Content layer should be accessible
    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(content_layer.rows(), 10);
    assert_eq!(content_layer.cols(), 80);
}

#[test]
fn test_compositor_status_bar_layer() {
    let mut system = RenderSystem::new(10, 80);

    // Status bar layer should be accessible
    let status_layer = system.compositor.get_layer_mut(LayerPriority::STATUS_BAR);
    assert_eq!(status_layer.rows(), 10);
    assert_eq!(status_layer.cols(), 80);
}

#[test]
fn test_compositor_floating_window_layer() {
    let mut system = RenderSystem::new(10, 80);

    // Floating window layer should be accessible
    let floating_layer = system
        .compositor
        .get_layer_mut(LayerPriority::FLOATING_WINDOW);
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
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(2, 11, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
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
    let mut state = State::new();
    state.settings.show_line_numbers = false;
    state.update_buffer_stats(1, 5, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
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
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(100, 0, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
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
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(10, 4, crate::document::LineEnding::LF); // 2 digits -> gutter 3

    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();
}

#[test]
fn test_no_redraw_on_noop() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("test").unwrap();
    let mut state = State::new();
    state.settings.show_line_numbers = false;
    let mut system = RenderSystem::new(10, 80);

    // 1. First render - populates layers and cache
    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: true,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();

    // Verify content was rendered
    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
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

    // 3. Second render - no state change
    system
        .render(
            &mut term,
            RenderState {
                buf: &buf,
                current_mode: Mode::Normal,
                pending_key: None,
                pending_count: 0,
                state: &state,
                needs_clear: false,
                tab_width: 4,
                highlights: None,
                modal: None,
            },
        )
        .unwrap();

    // Verify layer was NOT redrawn (cleared); should still show removal
    // Wait, RenderSystem::render uses self.render_cache.
    // If state is identical, it does nothing.
    // So 'X' should remain.
    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('X')
    );
}
