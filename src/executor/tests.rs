//! Tests for command executor

use crate::command::Command;
use crate::key::Key;
use crate::buffer::GapBuffer;
use crate::executor::execute_command;

#[test]
fn test_execute_move_left() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    assert_eq!(buf.cursor(), 5);
    
    execute_command(Command::MoveLeft, &mut buf, None);
    assert_eq!(buf.cursor(), 4);
}

#[test]
fn test_execute_move_right() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    // Move to start
    for _ in 0..5 {
        buf.move_left();
    }
    assert_eq!(buf.cursor(), 0);
    
    execute_command(Command::MoveRight, &mut buf, None);
    assert_eq!(buf.cursor(), 1);
}

#[test]
fn test_execute_insert_char() {
    let mut buf = GapBuffer::new(10).unwrap();
    execute_command(Command::InsertChar, &mut buf, Some(Key::Char(b'a')));
    assert_eq!(buf.to_string(), "a");
}

#[test]
fn test_execute_insert_newline() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    execute_command(Command::InsertNewline, &mut buf, None);
    assert_eq!(buf.to_string(), "hello\n");
}

#[test]
fn test_execute_backspace() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    execute_command(Command::Backspace, &mut buf, None);
    assert_eq!(buf.to_string(), "hell");
}

#[test]
fn test_execute_delete_char() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    // Move to start
    for _ in 0..5 {
        buf.move_left();
    }
    execute_command(Command::DeleteChar, &mut buf, None);
    assert_eq!(buf.to_string(), "ello");
}

#[test]
fn test_execute_move_to_buffer_start() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    assert_eq!(buf.cursor(), 5);
    
    execute_command(Command::MoveToBufferStart, &mut buf, None);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_execute_move_to_buffer_end() {
    let mut buf = GapBuffer::new(10).unwrap();
    buf.insert_str("hello").unwrap();
    // Move to start
    for _ in 0..5 {
        buf.move_left();
    }
    assert_eq!(buf.cursor(), 0);
    
    execute_command(Command::MoveToBufferEnd, &mut buf, None);
    assert_eq!(buf.cursor(), 5);
}

#[test]
fn test_execute_insert_ctrl_char() {
    let mut buf = GapBuffer::new(10).unwrap();
    // Ctrl+A should insert 0x01
    execute_command(Command::InsertChar, &mut buf, Some(Key::Ctrl(b'a')));
    let text = buf.to_string();
    assert_eq!(text.as_bytes()[0], 1); // Ctrl+A = 0x01
}

