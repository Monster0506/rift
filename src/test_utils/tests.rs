//! Tests for test utilities
//! These tests verify that MockTerminal works correctly

use crate::term::TerminalBackend;
use crate::test_utils::MockTerminal;

#[test]
fn test_mock_terminal_new() {
    let term = MockTerminal::new(10, 80);
    assert_eq!(term.size, (10, 80));
    assert_eq!(term.writes.len(), 0);
    assert_eq!(term.cursor_moves.len(), 0);
    assert_eq!(term.clear_screen_calls, 0);
}

#[test]
fn test_mock_terminal_write() {
    let mut term = MockTerminal::new(10, 80);
    term.write(b"hello").unwrap();
    term.write(b" world").unwrap();

    assert_eq!(term.writes.len(), 2);
    assert_eq!(term.writes[0], b"hello");
    assert_eq!(term.writes[1], b" world");
    assert_eq!(term.get_written_string(), "hello world");
}

#[test]
fn test_mock_terminal_get_written_bytes() {
    let mut term = MockTerminal::new(10, 80);
    term.write(b"test").unwrap();

    let bytes = term.get_written_bytes();
    assert_eq!(bytes, b"test");
}

#[test]
fn test_mock_terminal_get_written_string() {
    let mut term = MockTerminal::new(10, 80);
    term.write(b"hello").unwrap();
    term.write(b" world").unwrap();

    let written = term.get_written_string();
    assert_eq!(written, "hello world");
}

#[test]
fn test_mock_terminal_move_cursor() {
    let mut term = MockTerminal::new(10, 80);
    term.move_cursor(5, 10).unwrap();
    term.move_cursor(3, 20).unwrap();

    assert_eq!(term.cursor_moves.len(), 2);
    assert_eq!(term.cursor_moves[0], (5, 10));
    assert_eq!(term.cursor_moves[1], (3, 20));
}

#[test]
fn test_mock_terminal_clear_screen() {
    let mut term = MockTerminal::new(10, 80);
    term.clear_screen().unwrap();
    term.clear_screen().unwrap();

    assert_eq!(term.clear_screen_calls, 2);
}

#[test]
fn test_mock_terminal_clear() {
    let mut term = MockTerminal::new(10, 80);
    term.write(b"test").unwrap();
    term.move_cursor(1, 1).unwrap();
    term.clear_screen().unwrap();

    assert_eq!(term.writes.len(), 1);
    assert_eq!(term.cursor_moves.len(), 1);
    assert_eq!(term.clear_screen_calls, 1);

    term.clear();

    assert_eq!(term.writes.len(), 0);
    assert_eq!(term.cursor_moves.len(), 0);
    assert_eq!(term.clear_screen_calls, 0);
}

#[test]
fn test_mock_terminal_get_size() {
    let term = MockTerminal::new(24, 80);
    let size = term.get_size().unwrap();
    assert_eq!(size.rows, 24);
    assert_eq!(size.cols, 80);
}

#[test]
fn test_mock_terminal_init_deinit() {
    let mut term = MockTerminal::new(10, 80);
    // Should not panic
    term.init().unwrap();
    term.deinit();
}

#[test]
fn test_mock_terminal_read_key_not_implemented() {
    let mut term = MockTerminal::new(10, 80);
    let result = term.read_key();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Not implemented"));
}

#[test]
fn test_mock_terminal_cursor_operations() {
    let mut term = MockTerminal::new(10, 80);
    // Should not panic
    term.hide_cursor().unwrap();
    term.show_cursor().unwrap();
    term.clear_to_end_of_line().unwrap();
}

#[test]
fn test_mock_terminal_multiple_operations() {
    let mut term = MockTerminal::new(10, 80);

    term.write(b"line1").unwrap();
    term.move_cursor(1, 0).unwrap();
    term.write(b"line2").unwrap();
    term.clear_screen().unwrap();
    term.move_cursor(0, 0).unwrap();

    assert_eq!(term.writes.len(), 2);
    assert_eq!(term.cursor_moves.len(), 2);
    assert_eq!(term.clear_screen_calls, 1);
    assert_eq!(term.get_written_string(), "line1line2");
}
