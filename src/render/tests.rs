//! Tests for rendering system

use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::color::Color;
use crate::key::Key;
use crate::layer::Cell;
use crate::layer::{CellAttrs, CellStyle};
use crate::layer::{Layer, LayerPriority};
use crate::mode::Mode;
use crate::render::{
    calculate_cursor_column, calculate_cursor_column_at, CursorInfo, RenderState, RenderSystem,
    StatusDrawState,
};
use crate::state::State;
use crate::status::StatusBar;
use crate::test_utils::MockTerminal;

/// Remove ANSI escape sequences (CSI and OSC) from a string so tests can match
/// plain text content without being affected by color/cursor escape codes.
fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\x1b' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                    i += 1;
                }
                i += 1;
            } else {
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

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
        show_status_line: true,
        show_filename: true,
        show_dirty_indicator: true,
        search_match_index: None,
        search_total_matches: 0,
        cursor: CursorInfo { row: 0, col: 0 },
        lsp_status: None,
        lsp_ok_color: None,
        lsp_error_color: None,
        lsp_warn_color: None,
        is_remote: false,
    }
}
// Key formatting tests

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

// Cursor column calculation tests

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

// Status bar layer rendering tests

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

// Full render tests with compositor

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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();

    buf.insert_bytes(b"line1\nline2\nline3\n").unwrap();
    buf.move_to_start();

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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    // Should NOT clear screen on first render
    assert_eq!(term.clear_screen_calls, 0);

    let raw = term.get_written_string();
    let plain = strip_ansi(&raw);
    assert!(plain.contains("line1"), "expected 'line1' in: {plain:?}");
    assert!(plain.contains("line2"), "expected 'line2' in: {plain:?}");
    assert!(plain.contains("line3"), "expected 'line3' in: {plain:?}");

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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();
    // Should NOT clear when scrolling to show cursor at bottom (renderer logic attempts to minimize clears)
    assert_eq!(term.clear_screen_calls, 0);
    assert!(system.viewport.top_line() > 0);
}

// Layer content tests

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

// Line number rendering tests

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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    // Gutter width for 2 lines: 1 digit + 2 padding = 3, rendered " 1 "
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 1).unwrap().content,
        Character::from('1')
    );
    assert_eq!(
        content_layer.get_cell(0, 2).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 3).unwrap().content,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    // Gutter width: 3 digits + 2 padding = 5, rendered "   1 "
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
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 3).unwrap().content,
        Character::from('1')
    );
    assert_eq!(
        content_layer.get_cell(0, 4).unwrap().content,
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
    state.update_buffer_stats(10, 4, crate::document::LineEnding::LF); // 2 digits -> gutter 4

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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    // Identical state means render does nothing (render_cache), so the layer
    // is not redrawn and 'X' should remain.
    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('X')
    );
}

// Unicode cursor column tests

#[test]
fn test_cursor_column_wide_chars() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("a中b").unwrap();

    assert_eq!(calculate_cursor_column(&buf, 0, 4), 4);
    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 3);
    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 1);
    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 0);
}

#[test]
fn test_cursor_column_combining_chars() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("e\u{0301}").unwrap();

    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 1);
    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 0);
}

#[test]
fn test_cursor_column_truncated_utf8() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_bytes(&[0xE2, 0x80]).unwrap();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 8);
}

// Tab rendering tests

#[test]
fn test_tab_rendered_as_space_not_raw_tab() {
    // A tab is stored as space Cells so the screen buffer never writes a raw
    // tab byte (which would jump to the terminal's tab stop, not the editor's).
    let mut term = MockTerminal::new(5, 40);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("\thello").unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, 6, crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, 40);
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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: false,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    // The first 4 cells (tab expanded to 4 spaces) must all be spaces.
    for col in 0..4 {
        let cell = layer.get_cell(0, col).unwrap();
        assert_ne!(
            cell.content,
            Character::Tab,
            "col {col}: raw tab must not be stored in cell"
        );
        assert_eq!(
            cell.content,
            Character::from(' '),
            "col {col}: expanded tab cell should be a space"
        );
    }
    // Column 4 should be 'h'.
    assert_eq!(layer.get_cell(0, 4).unwrap().content, Character::from('h'));
}

#[test]
fn test_tab_straddling_left_col_does_not_shift_text() {
    // A tab straddling the left_col boundary must not push following chars right
    // by its full width: with tab_width=4 and left_col=2, 'h' sits at col 2, not 4.
    let mut term = MockTerminal::new(5, 40);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("\thello").unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, 6, crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, 40);

    // Scroll two columns to the right so the tab straddles left_col.
    system.viewport.set_scroll(0, 2);

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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: false,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    // With left_col=2 the tab's visible portion is 2 cells (screen cols 0-1);
    // 'h' (first char of "hello") must appear at screen col 2, not col 4.
    assert_eq!(
        layer.get_cell(0, 2).unwrap().content,
        Character::from('h'),
        "after partial tab, 'h' must be at screen col 2"
    );
}

#[test]
fn test_wide_char_straddling_left_col_renders_as_space() {
    // A CJK char (width 2) followed by "hello": with left_col=1 it straddles
    // the left edge and must render as a space, keeping later columns aligned.
    let mut term = MockTerminal::new(5, 40);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("你hello").unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, 6, crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, 40);

    system.viewport.set_scroll(0, 1);

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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: false,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        layer.get_cell(0, 0).unwrap().content,
        Character::from(' '),
        "partially-scrolled wide glyph must render as a space, not a clipped glyph"
    );
    assert_eq!(
        layer.get_cell(0, 1).unwrap().content,
        Character::from('h'),
        "'h' must follow directly after the straddling wide char, not be shifted"
    );
}

#[test]
fn test_zero_width_char_does_not_write_stray_cell() {
    // 'a' followed by a zero-width combining accent: the accent must not
    // write its own cell, which would corrupt whatever the terminal has there.
    let mut term = MockTerminal::new(5, 40);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("a\u{0301}").unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, 2, crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, 40);

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
                capture_map: None,
                injection_highlights: None,
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                custom_highlights: None,
                plugin_highlights: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                terminal_cell_colors: None,
                show_line_numbers: false,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        layer.get_cell(0, 0).unwrap().content,
        Character::from('a'),
        "'a' must remain in its own cell"
    );
    assert_ne!(
        layer.get_cell(0, 1).unwrap().content,
        Character::from('\u{0301}'),
        "a zero-width combining char must not write its own stray cell"
    );
}

// plan_glyph_draw: left-scroll-edge clipping decisions

#[test]
fn test_plan_glyph_draw_fully_visible() {
    use crate::render::plan_glyph_draw;
    let plan = plan_glyph_draw(1, 5, 0);
    assert_eq!(plan.visible_width, 1);
    assert!(!plan.straddles_left_edge);
}

#[test]
fn test_plan_glyph_draw_wide_char_straddling_left_edge() {
    use crate::render::plan_glyph_draw;
    // A width-2 glyph starting at visual col 0 with left_col=1 has only
    // 1 column visible: it straddles the edge and must render as a space.
    let plan = plan_glyph_draw(2, 0, 1);
    assert_eq!(plan.visible_width, 1);
    assert!(plan.straddles_left_edge);
}

#[test]
fn test_plan_glyph_draw_wide_char_fully_off_screen() {
    use crate::render::plan_glyph_draw;
    let plan = plan_glyph_draw(2, 0, 2);
    assert_eq!(plan.visible_width, 0);
    assert!(!plan.straddles_left_edge);
}

#[test]
fn test_plan_glyph_draw_zero_width_char_is_never_drawn() {
    use crate::render::plan_glyph_draw;
    let plan = plan_glyph_draw(0, 3, 0);
    assert_eq!(plan.visible_width, 0);
    assert!(!plan.straddles_left_edge);
}

// wrap_text unicode display-width tests

#[test]
fn test_wrap_text_cjk_counts_as_two_columns() {
    use crate::render::wrap_text;

    // The 2 CJK chars are each 2 columns wide (display width 4): they fit at
    // wrap_width=4 but at wrap_width=3 the 4-wide word wraps to its own line.
    let lines = wrap_text("你好", 4);
    assert_eq!(
        lines.len(),
        1,
        "2 CJK chars (display width 4) should fit in width 4"
    );
    assert_eq!(lines[0], "你好");

    // A word pushing total display width past the limit wraps: "AB" + space +
    // the 4-wide CJK word is 7 columns, exceeding width 5.
    let lines = wrap_text("AB 你好", 5);
    assert_eq!(
        lines.len(),
        2,
        "\"AB 你好\" should wrap at width 5 (display widths: 2+1+4=7 > 5)"
    );
    assert_eq!(lines[0], "AB");
    assert_eq!(lines[1], "你好");
}

// calculate_cursor_column_at: cursor column calculation

#[test]
fn test_cursor_column_at_matches_plain() {
    // calculate_cursor_column_at and calculate_cursor_column should agree.
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("hello").unwrap();
    assert_eq!(
        calculate_cursor_column_at(&buf, 0, 4, buf.cursor()),
        calculate_cursor_column(&buf, 0, 4),
    );
}

#[test]
fn test_cursor_column_at_mid_text() {
    // Column equals the number of chars before cursor.
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("abcde").unwrap();
    let _ = buf.set_cursor(3);

    assert_eq!(calculate_cursor_column_at(&buf, 0, 4, buf.cursor()), 3,);
}

// Annotation style hashing: must hash style fields directly, not via format!.

#[test]
fn test_cell_style_hash_no_alloc_and_distinguishes_styles() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // CellStyle must implement Hash directly (no Debug-string roundtrip).
    let style_a = CellStyle {
        fg: Some(Color::Red),
        bg: None,
        attrs: CellAttrs::default(),
    };
    let style_b = CellStyle {
        fg: Some(Color::Blue),
        bg: None,
        attrs: CellAttrs::default(),
    };

    let mut hasher_a1 = DefaultHasher::new();
    style_a.hash(&mut hasher_a1);
    let mut hasher_a2 = DefaultHasher::new();
    style_a.hash(&mut hasher_a2);
    let mut hasher_b = DefaultHasher::new();
    style_b.hash(&mut hasher_b);

    assert_eq!(
        hasher_a1.finish(),
        hasher_a2.finish(),
        "hashing the same style twice must be consistent"
    );
    assert_ne!(
        hasher_a1.finish(),
        hasher_b.finish(),
        "different styles must hash differently"
    );
}

// ContentDrawState: inline/adornment virtual text must affect redraw decisions.

fn inline_render_state<'a>(
    buf: &'a TextBuffer,
    state: &'a State,
    inline: &'a [(usize, usize, String, Color, bool)],
) -> RenderState<'a> {
    RenderState {
        buf,
        current_mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        state,
        needs_clear: false,
        tab_width: 4,
        highlights: None,
        capture_map: None,
        injection_highlights: None,
        skip_content: false,
        cursor_row_offset: 0,
        cursor_col_offset: 0,
        cursor_viewport: None,
        terminal_cursor: None,
        custom_highlights: None,
        plugin_highlights: None,
        annotation_styles: None,
        annotation_adornments: None,
        annotation_inline: Some(inline),
        annotation_concealed: None,
        terminal_cell_colors: None,
        show_line_numbers: false,
        display_map: None,
        scroll_hint: None,
    }
}

#[test]
fn test_inline_annotation_change_triggers_content_redraw() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    let state = State::new();
    let mut system = RenderSystem::new(10, 80);

    let inline_v1 = vec![(0usize, 0usize, "A".to_string(), Color::Red, true)];
    let inline_v2 = vec![(0usize, 0usize, "BB".to_string(), Color::Red, true)];

    system
        .render(&mut term, inline_render_state(&buf, &state, &inline_v1))
        .unwrap();
    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    let first_char_v1 = layer.get_cell(0, 0).unwrap().content;

    // Same revision, scroll, etc, only the inline virtual text changed.
    system
        .render(&mut term, inline_render_state(&buf, &state, &inline_v2))
        .unwrap();
    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    let first_char_v2 = layer.get_cell(0, 0).unwrap().content;

    assert_eq!(
        first_char_v1,
        Character::from('A'),
        "first inline annotation render should draw the leading virtual text"
    );
    assert_eq!(
        first_char_v2,
        Character::from('B'),
        "changing inline annotation text alone must trigger a content redraw"
    );
}

// highlights_hash: must cover all visible highlights and their capture index

fn render_state_with_highlights<'a>(
    buf: &'a TextBuffer,
    state: &'a State,
    highlights: &'a [(std::ops::Range<usize>, u32)],
) -> RenderState<'a> {
    RenderState {
        buf,
        current_mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        state,
        needs_clear: true,
        tab_width: 4,
        highlights: Some(highlights),
        capture_map: None,
        injection_highlights: None,
        skip_content: false,
        cursor_row_offset: 0,
        cursor_col_offset: 0,
        cursor_viewport: None,
        terminal_cursor: None,
        custom_highlights: None,
        plugin_highlights: None,
        annotation_styles: None,
        annotation_adornments: None,
        annotation_inline: None,
        annotation_concealed: None,
        terminal_cell_colors: None,
        show_line_numbers: false,
        display_map: None,
        scroll_hint: None,
    }
}

fn content_highlights_hash(system: &RenderSystem) -> u64 {
    use crate::render::components::Renderable;
    system
        .world
        .renderables
        .iter()
        .find_map(|(_, r)| match r {
            Renderable::TextBuffer(s) => Some(s.highlights_hash),
            _ => None,
        })
        .expect("content entity not found")
}

fn syntax_colors_state() -> State {
    let mut state = State::new();
    state.settings.syntax_colors = Some(crate::color::theme::SyntaxColors::from_base_colors(&[(
        "function",
        crate::color::Color::Red,
    )]));
    state
}

#[test]
fn test_highlights_hash_detects_change_beyond_take_16_cap() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(1000).unwrap();
    let state = syntax_colors_state();
    let mut system = RenderSystem::new(10, 80);

    let mut base: Vec<(std::ops::Range<usize>, u32)> =
        (0..20).map(|i| (i * 10..i * 10 + 5, 1)).collect();
    system
        .render(&mut term, render_state_with_highlights(&buf, &state, &base))
        .unwrap();
    let hash_before = content_highlights_hash(&system);

    // Only the range at index 17 (beyond the old take(16) cap) changes.
    base[17] = (170..200, 1);
    system
        .render(&mut term, render_state_with_highlights(&buf, &state, &base))
        .unwrap();
    let hash_after = content_highlights_hash(&system);

    assert_ne!(
        hash_before, hash_after,
        "changing a highlight range beyond index 16 must change highlights_hash"
    );
}

#[test]
fn test_highlights_hash_detects_capture_change_on_same_range() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(1000).unwrap();
    let state = syntax_colors_state();
    let mut system = RenderSystem::new(10, 80);

    let mut base: Vec<(std::ops::Range<usize>, u32)> =
        (0..20).map(|i| (i * 10..i * 10 + 5, 1)).collect();
    system
        .render(&mut term, render_state_with_highlights(&buf, &state, &base))
        .unwrap();
    let hash_before = content_highlights_hash(&system);

    // Same byte ranges throughout, only the capture index at index 0 changes.
    base[0].1 = 2;
    system
        .render(&mut term, render_state_with_highlights(&buf, &state, &base))
        .unwrap();
    let hash_after = content_highlights_hash(&system);

    assert_ne!(
        hash_before, hash_after,
        "changing only a highlight's capture index must change highlights_hash"
    );
}
