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
    buf.insert(b'a').unwrap();
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.cursor(), 1);
}

#[test]
fn test_move_and_insert() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    // Move to start
    for _ in 0..5 {
        buf.move_left();
    }
    buf.insert(b'X').unwrap();
    assert_eq!(buf.to_string(), "Xhello");
}

#[test]
fn test_delete() {
    // Test deleting at cursor position (backspace)
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    // Cursor at end, delete_backward should delete 'o'
    assert!(buf.delete_backward());
    assert_eq!(buf.to_string(), "hell");
    
    // Test deleting after moving cursor
    let mut buf2 = GapBuffer::new(10).unwrap();
    buf2.insert_str("hello").unwrap();
    // Move cursor left (before 'o')
    assert!(buf2.move_left());
    // delete_backward deletes byte before cursor ('l')
    assert!(buf2.delete_backward());
    assert_eq!(buf2.to_string(), "helo");
}

#[test]
fn test_delete_at_start() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    // Move to start
    for _ in 0..5 {
        buf.move_left();
    }
    // Can't delete at start
    assert!(!buf.delete_backward());
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn test_delete_forward() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    // Move to start
    for _ in 0..5 {
        buf.move_left();
    }
    // Delete forward should delete 'h'
    assert!(buf.delete_forward());
    assert_eq!(buf.to_string(), "ello");
}

#[test]
fn test_move_right() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    // Move to start
    for _ in 0..5 {
        buf.move_left();
    }
    // Move right once
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
    buf.insert(b'h').unwrap();
    buf.insert(b'e').unwrap();
    buf.insert(b'l').unwrap();
    buf.insert(b'l').unwrap();
    buf.insert(b'o').unwrap();
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
    // Fill the gap
    buf.insert_str("abcd").unwrap();
    // This should trigger growth
    buf.insert(b'e').unwrap();
    assert_eq!(buf.to_string(), "abcde");
    // Capacity is private, just verify it works
    assert_eq!(buf.len(), 5);
}

