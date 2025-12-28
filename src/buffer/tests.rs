//! Tests for gap buffer implementation

use crate::buffer::GapBuffer;

#[test]
fn test_new() {
    let buf = GapBuffer::new(10).unwrap();
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_insert() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_char('a').unwrap();
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.cursor(), 1);
}

#[test]
fn test_move_and_insert() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    for _ in 0..5 {
        buf.move_left();
    }
    buf.insert_char('X').unwrap();
    assert_eq!(buf.to_string(), "Xhello");
}

#[test]
fn test_delete() {
    // Test deleting at cursor position (backspace)
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    assert!(buf.delete_backward());
    assert_eq!(buf.to_string(), "hell");

    // Test deleting after moving cursor
    let mut buf2 = GapBuffer::new(10).unwrap();
    buf2.insert_str("hello").unwrap();
    assert!(buf2.move_left());
    assert!(buf2.delete_backward());
    assert_eq!(buf2.to_string(), "helo");
}

#[test]
fn test_delete_at_start() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    for _ in 0..5 {
        buf.move_left();
    }
    assert!(!buf.delete_backward());
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn test_delete_forward() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    for _ in 0..5 {
        buf.move_left();
    }
    assert!(buf.delete_forward());
    assert_eq!(buf.to_string(), "ello");
}

#[test]
fn test_move_right() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    for _ in 0..5 {
        buf.move_left();
    }
    assert!(buf.move_right());
    assert_eq!(buf.cursor(), 1);
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn test_cursor_position() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    assert_eq!(buf.cursor(), 5);
    assert!(buf.move_left());
    assert_eq!(buf.cursor(), 4);
    assert!(buf.move_left());
    assert_eq!(buf.cursor(), 3);
}

#[test]
fn test_multiple_inserts() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_char('h').unwrap();
    buf.insert_char('e').unwrap();
    buf.insert_char('l').unwrap();
    buf.insert_char('l').unwrap();
    buf.insert_char('o').unwrap();
    assert_eq!(buf.to_string(), "hello");
    assert_eq!(buf.len(), 5);
}

#[test]
fn test_delete_backward_multiple() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    assert!(buf.delete_backward());
    assert_eq!(buf.to_string(), "hell");
    assert!(buf.delete_backward());
    assert_eq!(buf.to_string(), "hel");
    assert!(buf.delete_backward());
    assert_eq!(buf.to_string(), "he");
}

#[test]
fn test_gap_expansion() {
    let mut buf = GapBuffer::new(4).unwrap();
    buf.insert_str("abcd").unwrap();
    buf.insert_char('e').unwrap();
    assert_eq!(buf.to_string(), "abcde");
    assert_eq!(buf.len(), 5);
}

#[test]
fn test_insert_bytes() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_bytes(b"hello").unwrap();
    assert_eq!(buf.to_string(), "hello");
    assert_eq!(buf.len(), 5);
    assert_eq!(buf.cursor(), 5);
}

#[test]
fn test_insert_bytes_empty() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_bytes(b"").unwrap();
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_insert_bytes_large() {
    let mut buf = GapBuffer::new(10).unwrap();
    let large_text = "a".repeat(1000);
    buf.insert_bytes(large_text.as_bytes()).unwrap();
    assert_eq!(buf.len(), 1000);
    assert_eq!(buf.to_string(), large_text);
}

#[test]
fn test_insert_bytes_with_newlines() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_bytes(b"hello\nworld\n").unwrap();
    assert_eq!(buf.to_string(), "hello\nworld\n");
    assert_eq!(buf.len(), 12);
}

#[test]
fn test_line_indexing_basic() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("line1\nline2\nline3").unwrap();
    assert_eq!(buf.get_total_lines(), 3);
    assert_eq!(buf.get_line(), 2); // Cursor at end of line 3 (index 2)
}

#[test]
fn test_line_indexing_bytes() {
    let mut buf = GapBuffer::new(20).unwrap();
    buf.insert_str("abc\ndef\nghi").unwrap();
    assert_eq!(buf.get_line_bytes(0), b"abc");
    assert_eq!(buf.get_line_bytes(1), b"def");
    assert_eq!(buf.get_line_bytes(2), b"ghi");
}

#[test]
fn test_line_indexing_with_gap() {
    let mut buf = GapBuffer::new(20).unwrap();
    buf.insert_str("hello\nworld").unwrap();

    // Move cursor to middle of "hello"
    for _ in 0..8 {
        buf.move_left();
    }
    // Buffer: [he] <gap> [llo\nworld]

    assert_eq!(buf.get_line_bytes(0), b"hello");
    assert_eq!(buf.get_line_bytes(1), b"world");
}

#[test]
fn test_line_indexing_delete_merge() {
    let mut buf = GapBuffer::new(20).unwrap();
    buf.insert_str("abc\ndef").unwrap();
    assert_eq!(buf.get_total_lines(), 2);

    // Move to newline and delete it
    for _ in 0..4 {
        buf.move_left();
    }
    assert!(buf.delete_forward()); // Delete '\n'

    assert_eq!(buf.to_string(), "abcdef");
    assert_eq!(buf.get_total_lines(), 1);
    assert_eq!(buf.get_line_bytes(0), b"abcdef");
}

#[test]
fn test_line_indexing_complex_insert() {
    let mut buf = GapBuffer::new(20).unwrap();
    buf.insert_str("a\nd").unwrap();
    // Move to after 'a'
    for _ in 0..2 {
        buf.move_left();
    }
    buf.insert_str("b\nc").unwrap();

    assert_eq!(buf.to_string(), "ab\nc\nd");
    assert_eq!(buf.get_total_lines(), 3);
    assert_eq!(buf.get_line_bytes(0), b"ab");
    assert_eq!(buf.get_line_bytes(1), b"c");
    assert_eq!(buf.get_line_bytes(2), b"d");
}
