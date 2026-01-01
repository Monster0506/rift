//! Tests for command executor

use crate::action::Motion;
use crate::command::Command;
use crate::document::Document;
use crate::executor::execute_command;

fn create_doc() -> Document {
    Document::new(1).unwrap()
}

#[test]
fn test_execute_move_left() {
    let mut doc = create_doc();
    doc.buffer.insert_str("hello").unwrap();
    assert_eq!(doc.buffer.cursor(), 5);

    execute_command(Command::Move(Motion::Left, 1), &mut doc, false, 8, 24, None).unwrap();
    assert_eq!(doc.buffer.cursor(), 4);
}

#[test]
fn test_execute_move_right() {
    let mut doc = create_doc();
    doc.buffer.insert_str("hello").unwrap();
    for _ in 0..5 {
        doc.buffer.move_left();
    }
    assert_eq!(doc.buffer.cursor(), 0);

    execute_command(
        Command::Move(Motion::Right, 1),
        &mut doc,
        false,
        8,
        24,
        None,
    )
    .unwrap();
    assert_eq!(doc.buffer.cursor(), 1);
}

#[test]
fn test_execute_insert_char() {
    let mut doc = create_doc();
    execute_command(Command::InsertChar('a'), &mut doc, false, 8, 24, None).unwrap();
    assert_eq!(doc.buffer.to_string(), "a");
}

#[test]
fn test_execute_insert_newline() {
    let mut doc = create_doc();
    doc.buffer.insert_str("hello").unwrap();
    execute_command(Command::InsertChar('\n'), &mut doc, false, 8, 24, None).unwrap();
    assert_eq!(doc.buffer.to_string(), "hello\n");
}

#[test]
fn test_execute_delete_backward() {
    let mut doc = create_doc();
    doc.buffer.insert_str("hello").unwrap();
    execute_command(Command::DeleteBackward, &mut doc, false, 8, 24, None).unwrap();
    assert_eq!(doc.buffer.to_string(), "hell");
}

#[test]
fn test_execute_delete_forward() {
    let mut doc = create_doc();
    doc.buffer.insert_str("hello").unwrap();
    for _ in 0..5 {
        doc.buffer.move_left();
    }
    execute_command(Command::DeleteForward, &mut doc, false, 8, 24, None).unwrap();
    assert_eq!(doc.buffer.to_string(), "ello");
}

#[test]
fn test_execute_move_to_buffer_start() {
    let mut doc = create_doc();
    doc.buffer.insert_str("hello").unwrap();
    assert_eq!(doc.buffer.cursor(), 5);

    execute_command(
        Command::Move(Motion::StartOfFile, 1),
        &mut doc,
        false,
        8,
        24,
        None,
    )
    .unwrap();
    assert_eq!(doc.buffer.cursor(), 0);
}

#[test]
fn test_execute_move_to_buffer_end() {
    let mut doc = create_doc();
    doc.buffer.insert_str("hello").unwrap();
    for _ in 0..5 {
        doc.buffer.move_left();
    }
    assert_eq!(doc.buffer.cursor(), 0);

    execute_command(
        Command::Move(Motion::EndOfFile, 1),
        &mut doc,
        false,
        8,
        24,
        None,
    )
    .unwrap();
    assert_eq!(doc.buffer.cursor(), 5);
}

#[test]
fn test_execute_insert_ctrl_char() {
    let mut doc = create_doc();
    // Ctrl+A should insert \u{1}
    execute_command(Command::InsertChar('\u{1}'), &mut doc, false, 8, 24, None).unwrap();
    let text = doc.buffer.to_string();
    assert_eq!(text.as_bytes()[0], 1); // Ctrl+A = 1
}

#[test]
fn test_execute_insert_tab_expanded_at_column_0() {
    let mut doc = create_doc();
    execute_command(Command::InsertChar('\t'), &mut doc, true, 8, 24, None).unwrap();
    let text = doc.buffer.to_string();
    assert_eq!(text, "        "); // 8 spaces
    assert_eq!(text.len(), 8);
}

#[test]
fn test_execute_insert_tab_expanded_at_column_1() {
    let mut doc = create_doc();
    doc.buffer.insert_str("a").unwrap();
    execute_command(Command::InsertChar('\t'), &mut doc, true, 8, 24, None).unwrap();
    let text = doc.buffer.to_string();
    assert_eq!(text, "a       "); // 1 char + 7 spaces
    assert_eq!(text.len(), 8);
}

#[test]
fn test_execute_insert_tab_expanded_at_column_7() {
    let mut doc = create_doc();
    doc.buffer.insert_str("abcdefg").unwrap();
    execute_command(Command::InsertChar('\t'), &mut doc, true, 8, 24, None).unwrap();
    let text = doc.buffer.to_string();
    assert_eq!(text, "abcdefg "); // 7 chars + 1 space
    assert_eq!(text.len(), 8);
}

#[test]
fn test_execute_insert_tab_expanded_at_column_8() {
    let mut doc = create_doc();
    doc.buffer.insert_str("abcdefgh").unwrap();
    execute_command(Command::InsertChar('\t'), &mut doc, true, 8, 24, None).unwrap();
    let text = doc.buffer.to_string();
    assert_eq!(text, "abcdefgh        "); // 8 chars + 8 spaces
    assert_eq!(text.len(), 16);
}

#[test]
fn test_execute_insert_tab_not_expanded() {
    let mut doc = create_doc();
    execute_command(Command::InsertChar('\t'), &mut doc, false, 8, 24, None).unwrap();
    let text = doc.buffer.to_string();
    assert_eq!(text, "\t");
    assert_eq!(text.len(), 1);
    assert_eq!(text.as_bytes()[0], b'\t');
}
