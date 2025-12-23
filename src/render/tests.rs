//! Tests for rendering system

use crate::render::{render, render_content, render_status_bar, format_key, calculate_cursor_column};
use crate::buffer::GapBuffer;
use crate::mode::Mode;
use crate::key::Key;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use crate::state::State;

/// Mock terminal backend for testing
/// Records all operations for verification
pub struct MockTerminal {
    pub writes: Vec<Vec<u8>>,
    pub cursor_moves: Vec<(u16, u16)>,
    pub clear_screen_calls: usize,
    pub size: (u16, u16),
}

impl MockTerminal {
    pub fn new(rows: u16, cols: u16) -> Self {
        MockTerminal {
            writes: Vec::new(),
            cursor_moves: Vec::new(),
            clear_screen_calls: 0,
            size: (rows, cols),
        }
    }

    pub fn get_written_bytes(&self) -> Vec<u8> {
        self.writes.iter().flatten().cloned().collect()
    }

    pub fn get_written_string(&self) -> String {
        String::from_utf8_lossy(&self.get_written_bytes()).to_string()
    }
}

impl TerminalBackend for MockTerminal {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn deinit(&mut self) {}

    fn read_key(&mut self) -> Result<Key, String> {
        Err("Not implemented in mock".to_string())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writes.push(bytes.to_vec());
        Ok(())
    }

    fn get_size(&self) -> Result<crate::term::Size, String> {
        Ok(crate::term::Size {
            rows: self.size.0,
            cols: self.size.1,
        })
    }

    fn clear_screen(&mut self) -> Result<(), String> {
        self.clear_screen_calls += 1;
        Ok(())
    }

    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String> {
        self.cursor_moves.push((row, col));
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn test_format_key_char() {
    assert_eq!(format_key(Key::Char(b'a')), "a");
    assert_eq!(format_key(Key::Char(b'Z')), "Z");
    assert_eq!(format_key(Key::Char(b' ')), " ");
    assert_eq!(format_key(Key::Char(b'0')), "0");
}

#[test]
fn test_format_key_non_printable() {
    assert_eq!(format_key(Key::Char(0x00)), "\\x00");
    assert_eq!(format_key(Key::Char(0x1F)), "\\x1f");
    assert_eq!(format_key(Key::Char(0x7F)), "\\x7f");
}

#[test]
fn test_format_key_ctrl() {
    assert_eq!(format_key(Key::Ctrl(b'a')), "Ctrl+A");
    assert_eq!(format_key(Key::Ctrl(b'c')), "Ctrl+C");
    assert_eq!(format_key(Key::Ctrl(b'z')), "Ctrl+Z");
}

#[test]
fn test_format_key_arrows() {
    assert_eq!(format_key(Key::ArrowUp), "↑");
    assert_eq!(format_key(Key::ArrowDown), "↓");
    assert_eq!(format_key(Key::ArrowLeft), "←");
    assert_eq!(format_key(Key::ArrowRight), "→");
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

#[test]
fn test_calculate_cursor_column_single_line() {
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    // Cursor is at position 5 (after "hello")
    assert_eq!(calculate_cursor_column(&buf, 0), 5);
}

#[test]
fn test_calculate_cursor_column_multiline() {
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2\nline3").unwrap();
    // Move to start
    for _ in 0..18 {
        buf.move_left();
    }
    // Now cursor is at start of line 0
    assert_eq!(calculate_cursor_column(&buf, 0), 0);
    
    // Move to line 1
    buf.move_down();
    assert_eq!(calculate_cursor_column(&buf, 1), 0);
    
    // Move right 3 times on line 1
    buf.move_right();
    buf.move_right();
    buf.move_right();
    assert_eq!(calculate_cursor_column(&buf, 1), 3);
}

#[test]
fn test_calculate_cursor_column_empty_buffer() {
    let buf = GapBuffer::new(100).unwrap();
    assert_eq!(calculate_cursor_column(&buf, 0), 0);
}

#[test]
fn test_calculate_cursor_column_at_gap() {
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    // Move cursor to middle
    for _ in 0..3 {
        buf.move_left();
    }
    // Cursor should be at column 2
    assert_eq!(calculate_cursor_column(&buf, 0), 2);
}

#[test]
fn test_render_content_empty_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let buf = GapBuffer::new(100).unwrap();
    let viewport = Viewport::new(10, 80);
    
    render_content(&mut term, &buf, &viewport).unwrap();
    
    // Should write empty lines for visible rows
    assert!(term.writes.len() > 0);
}

#[test]
fn test_render_content_single_line() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("hello world").unwrap();
    let viewport = Viewport::new(10, 80);
    
    render_content(&mut term, &buf, &viewport).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("hello world"));
}

#[test]
fn test_render_content_multiline() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2\nline3").unwrap();
    let viewport = Viewport::new(10, 80);
    
    render_content(&mut term, &buf, &viewport).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("line1"));
    assert!(written.contains("line2"));
    assert!(written.contains("line3"));
}

#[test]
fn test_render_content_line_truncation() {
    let mut term = MockTerminal::new(10, 10);
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("this is a very long line").unwrap();
    let viewport = Viewport::new(10, 10);
    
    render_content(&mut term, &buf, &viewport).unwrap();
    
    // Check that lines are truncated to viewport width
    let written = term.get_written_bytes();
    // Find the line content (excluding \r\n)
    for write in &term.writes {
        if write.len() >= 10 && write[0] != b'\r' {
            // Line should be exactly 10 bytes (viewport width)
            assert_eq!(write.len(), 10);
        }
    }
}

#[test]
fn test_render_content_line_padding() {
    let mut term = MockTerminal::new(10, 20);
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("short").unwrap();
    let viewport = Viewport::new(10, 20);
    
    render_content(&mut term, &buf, &viewport).unwrap();
    
    // Check that short lines are padded with spaces
    let written = term.get_written_bytes();
    // Should have padding spaces
    assert!(written.contains(&b' '));
}

#[test]
fn test_render_status_bar_normal_mode() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render_status_bar(&mut term, &viewport, Mode::Normal, None, &state).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("NORMAL"));
    assert!(!written.contains("INSERT"));
}

#[test]
fn test_render_status_bar_insert_mode() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render_status_bar(&mut term, &viewport, Mode::Insert, None, &state).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("INSERT"));
    assert!(!written.contains("NORMAL"));
}

#[test]
fn test_render_status_bar_pending_key() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render_status_bar(&mut term, &viewport, Mode::Normal, Some(Key::Char(b'd')), &state).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("[d]"));
}

#[test]
fn test_render_status_bar_debug_mode() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.toggle_debug();
    state.update_keypress(Key::Char(b'a'));
    state.update_cursor(5, 10);
    state.update_buffer_stats(10, 100);
    
    render_status_bar(&mut term, &viewport, Mode::Normal, None, &state).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("Last: a"));
    assert!(written.contains("Pos: 6:11")); // 1-indexed
    assert!(written.contains("Lines: 10"));
    assert!(written.contains("Size: 100B"));
}

#[test]
fn test_render_status_bar_debug_mode_with_pending_key() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let mut state = State::new();
    state.toggle_debug();
    state.update_keypress(Key::ArrowUp);
    
    render_status_bar(&mut term, &viewport, Mode::Normal, Some(Key::Char(b'd')), &state).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("NORMAL"));
    assert!(written.contains("[d]"));
    assert!(written.contains("Last: ↑"));
}

#[test]
fn test_render_status_bar_fills_line() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render_status_bar(&mut term, &viewport, Mode::Normal, None, &state).unwrap();
    
    // Status bar should fill the entire line width
    let total_written: usize = term.writes.iter().map(|w| w.len()).sum();
    // Should write at least the viewport width (accounting for mode string and padding)
    assert!(total_written >= 80);
}

#[test]
fn test_render_status_bar_reverse_video() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render_status_bar(&mut term, &viewport, Mode::Normal, None, &state).unwrap();
    
    let _written = term.get_written_bytes();
    // Should contain reverse video escape sequence
    assert!(!term.writes.is_empty());
    let written_str = term.get_written_string();
    assert!(written_str.contains("\x1b[7m")); // Reverse video
    assert!(written_str.contains("\x1b[0m")); // Reset
}

#[test]
fn test_render_clears_screen() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    let mut viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
    
    assert_eq!(term.clear_screen_calls, 1);
}

#[test]
fn test_render_cursor_positioning() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    let mut viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
    
    // Should have moved cursor
    assert!(!term.cursor_moves.is_empty());
}

#[test]
fn test_render_empty_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let buf = GapBuffer::new(100).unwrap();
    let mut viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
    
    assert_eq!(term.clear_screen_calls, 1);
    // Should still render empty lines
    assert!(!term.writes.is_empty());
}

#[test]
fn test_render_multiline_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2\nline3\nline4\nline5").unwrap();
    let mut viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("line1"));
    assert!(written.contains("line2"));
    assert!(written.contains("line3"));
}

#[test]
fn test_render_viewport_scrolling() {
    let mut term = MockTerminal::new(5, 80); // Small viewport
    let mut buf = GapBuffer::new(100).unwrap();
    // Create 10 lines
    for i in 0..10 {
        buf.insert_str(&format!("line{}\n", i)).unwrap();
    }
    // Move cursor to line 8
    for _ in 0..8 {
        buf.move_up();
    }
    let mut viewport = Viewport::new(5, 80);
    let state = State::new();
    
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
    
    // Viewport should scroll to show cursor
    // Top line should be adjusted
    assert!(viewport.top_line() <= 8);
}

#[test]
fn test_render_status_bar_debug_truncation() {
    let mut term = MockTerminal::new(10, 20); // Narrow viewport
    let viewport = Viewport::new(10, 20);
    let mut state = State::new();
    state.toggle_debug();
    state.update_keypress(Key::Char(b'a'));
    state.update_cursor(100, 200);
    state.update_buffer_stats(1000, 50000);
    
    render_status_bar(&mut term, &viewport, Mode::Normal, None, &state).unwrap();
    
    // Debug info should be truncated if too long
    let written = term.get_written_string();
    // Should still contain mode
    assert!(written.contains("NORMAL"));
}

#[test]
fn test_render_content_empty_lines() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("line1\n\nline3").unwrap();
    let viewport = Viewport::new(10, 80);
    
    render_content(&mut term, &buf, &viewport).unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains("line1"));
    assert!(written.contains("line3"));
}

#[test]
fn test_render_content_only_newlines() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("\n\n\n").unwrap();
    let viewport = Viewport::new(10, 80);
    
    render_content(&mut term, &buf, &viewport).unwrap();
    
    // Should render empty lines
    assert!(!term.writes.is_empty());
}

#[test]
fn test_calculate_cursor_column_multiline_complex() {
    let mut buf = GapBuffer::new(100).unwrap();
    buf.insert_str("hello\nworld\ntest").unwrap();
    
    // Move to start
    for _ in 0..17 {
        buf.move_left();
    }
    assert_eq!(calculate_cursor_column(&buf, 0), 0);
    
    // Move to end of first line
    for _ in 0..5 {
        buf.move_right();
    }
    assert_eq!(calculate_cursor_column(&buf, 0), 5);
    
    // Move to next line
    buf.move_right(); // Move past newline
    assert_eq!(calculate_cursor_column(&buf, 1), 0);
    
    // Move to middle of second line
    for _ in 0..3 {
        buf.move_right();
    }
    assert_eq!(calculate_cursor_column(&buf, 1), 3);
}

#[test]
fn test_render_status_bar_all_modes() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    
    // Test Normal mode
    render_status_bar(&mut term, &viewport, Mode::Normal, None, &state).unwrap();
    let written_normal = term.get_written_string();
    assert!(written_normal.contains("NORMAL"));
    
    // Reset and test Insert mode
    term.writes.clear();
    render_status_bar(&mut term, &viewport, Mode::Insert, None, &state).unwrap();
    let written_insert = term.get_written_string();
    assert!(written_insert.contains("INSERT"));
}

#[test]
fn test_render_status_bar_various_keys() {
    let mut term = MockTerminal::new(10, 80);
    let viewport = Viewport::new(10, 80);
    let state = State::new();
    
    // Test various pending keys
    let keys = vec![
        Key::Char(b'a'),
        Key::ArrowUp,
        Key::Ctrl(b'c'),
        Key::Escape,
    ];
    
    for key in keys {
        term.writes.clear();
        render_status_bar(&mut term, &viewport, Mode::Normal, Some(key), &state).unwrap();
        let written = term.get_written_string();
        assert!(written.contains("["));
        assert!(written.contains("]"));
    }
}

#[test]
fn test_render_content_unicode_safety() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    // Insert bytes that might be part of multi-byte UTF-8
    buf.insert(0xC3).unwrap();
    buf.insert(0xA9).unwrap(); // é in UTF-8
    let viewport = Viewport::new(10, 80);
    
    // Should not panic
    render_content(&mut term, &buf, &viewport).unwrap();
    assert!(!term.writes.is_empty());
}

#[test]
fn test_render_viewport_edge_cases() {
    let mut term = MockTerminal::new(1, 1); // Minimal viewport
    let buf = GapBuffer::new(100).unwrap();
    let mut viewport = Viewport::new(1, 1);
    let state = State::new();
    
    // Should not panic with minimal viewport
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
}

#[test]
fn test_render_large_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = GapBuffer::new(10000).unwrap();
    // Insert a large amount of text
    for i in 0..100 {
        buf.insert_str(&format!("line {}\n", i)).unwrap();
    }
    let mut viewport = Viewport::new(10, 80);
    let state = State::new();
    
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
    
    // Should render successfully
    assert_eq!(term.clear_screen_calls, 1);
    assert!(!term.writes.is_empty());
}

#[test]
fn test_render_cursor_at_viewport_boundaries() {
    let mut term = MockTerminal::new(5, 80);
    let mut buf = GapBuffer::new(100).unwrap();
    // Create content
    for i in 0..20 {
        buf.insert_str(&format!("line {}\n", i)).unwrap();
    }
    let mut viewport = Viewport::new(5, 80);
    let state = State::new();
    
    // Test cursor at top
    for _ in 0..20 {
        buf.move_up();
    }
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
    assert_eq!(term.clear_screen_calls, 1);
    
    // Reset
    term.clear_screen_calls = 0;
    term.cursor_moves.clear();
    term.writes.clear();
    
    // Test cursor at bottom
    for _ in 0..20 {
        buf.move_down();
    }
    render(&mut term, &buf, &mut viewport, Mode::Normal, None, &state).unwrap();
    assert_eq!(term.clear_screen_calls, 1);
}

