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
    // Verify using char_at because to_string() uses Display which renders ^A for control chars
    use crate::character::Character;
    assert_eq!(doc.buffer.char_at(0), Some(Character::Control(1)));
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

// =============================================================================
// Undo/Redo Executor Tests
// =============================================================================

#[test]
fn test_execute_undo_command() {
    let mut doc = create_doc();

    // Insert something
    doc.insert_char('x').unwrap();
    assert_eq!(doc.buffer.to_string(), "x");

    // Execute undo command
    execute_command(Command::Undo, &mut doc, false, 8, 24, None).unwrap();
    assert_eq!(doc.buffer.to_string(), "");
}

#[test]
fn test_execute_redo_command() {
    let mut doc = create_doc();

    // Insert and undo
    doc.insert_char('y').unwrap();
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");

    // Execute redo command
    execute_command(Command::Redo, &mut doc, false, 8, 24, None).unwrap();
    assert_eq!(doc.buffer.to_string(), "y");
}

#[test]
fn test_execute_delete_line_single_undo() {
    let mut doc = create_doc();

    // Add a line
    doc.buffer.insert_str("hello world\n").unwrap();
    doc.buffer.move_to_start();
    assert_eq!(doc.buffer.to_string(), "hello world\n");

    // Wrap delete line in transaction (simulating normal mode behavior)
    doc.begin_transaction("DeleteLine");
    execute_command(Command::DeleteLine, &mut doc, false, 8, 24, None).unwrap();
    doc.commit_transaction();

    assert_eq!(doc.buffer.to_string(), "");

    // Single undo should restore the entire line
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "hello world\n");
}

#[test]
fn test_execute_delete_motion_single_undo() {
    let mut doc = create_doc();

    // Add text
    doc.buffer.insert_str("one two three").unwrap();
    doc.buffer.move_to_start();
    assert_eq!(doc.buffer.cursor(), 0);

    // Wrap delete word in transaction (simulating d2w)
    doc.begin_transaction("Delete(NextWord, 2)");
    execute_command(
        Command::Delete(Motion::NextWord, 2),
        &mut doc,
        false,
        8,
        24,
        None,
    )
    .unwrap();
    doc.commit_transaction();

    // "one two " should be deleted
    assert_eq!(doc.buffer.to_string(), "three");

    // Single undo should restore
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "one two three");
}

#[test]
fn test_insert_mode_transaction_simulation() {
    let mut doc = create_doc();

    // Simulate entering insert mode
    doc.begin_transaction("Insert");

    // Type multiple characters (simulating insert mode typing)
    doc.insert_char('a').unwrap();
    doc.insert_char('b').unwrap();
    doc.insert_char('c').unwrap();

    // Simulate exiting insert mode
    doc.commit_transaction();

    assert_eq!(doc.buffer.to_string(), "abc");

    // ONE undo should remove ALL characters (grouped as single transaction)
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");

    // ONE redo should restore ALL characters
    doc.redo();
    assert_eq!(doc.buffer.to_string(), "abc");
}

#[test]
fn test_multiple_insert_sessions() {
    let mut doc = create_doc();

    // First insert mode session
    doc.begin_transaction("Insert 1");
    doc.insert_char('X').unwrap();
    doc.insert_char('Y').unwrap();
    doc.commit_transaction();

    // Second insert mode session
    doc.begin_transaction("Insert 2");
    doc.insert_char('Z').unwrap();
    doc.commit_transaction();

    assert_eq!(doc.buffer.to_string(), "XYZ");

    // Undo second session
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "XY");

    // Undo first session
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");
}

#[test]
fn test_undo_then_new_insert_creates_branch() {
    let mut doc = create_doc();

    // First insert
    doc.begin_transaction("Insert A");
    doc.insert_char('A').unwrap();
    doc.commit_transaction();

    // Second insert
    doc.begin_transaction("Insert B");
    doc.insert_char('B').unwrap();
    doc.commit_transaction();

    assert_eq!(doc.buffer.to_string(), "AB");

    // Undo B
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "A");

    // New insert (creates branch)
    doc.begin_transaction("Insert C");
    doc.insert_char('C').unwrap();
    doc.commit_transaction();

    assert_eq!(doc.buffer.to_string(), "AC");

    // Undo C
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "A");

    // Redo goes to last visited branch (C, not B)
    doc.redo();
    assert_eq!(doc.buffer.to_string(), "AC");
}

#[test]
fn test_execute_delete_16_lines_down() {
    let mut doc = create_doc();

    // Create 20 lines
    for i in 0..20 {
        doc.buffer.insert_str(&format!("line {}\n", i)).unwrap();
    }
    doc.buffer.move_to_start();

    // Verify we have 20 lines (plus potential trailing content)
    let initial_text = doc.buffer.to_string();
    let initial_line_count = initial_text.lines().count();
    assert_eq!(initial_line_count, 20);

    // Execute d16j (delete with motion Down, count 16)
    // This should delete from line 0 down to line 16 (17 lines total: current + 16 below)
    execute_command(
        Command::Delete(Motion::Down, 16),
        &mut doc,
        false,
        8,
        24,
        None,
    )
    .unwrap();

    let after_delete = doc.buffer.to_string();
    let remaining_lines: Vec<&str> = after_delete.lines().collect();

    // Should have lines 17, 18, 19 remaining (3 lines)
    assert_eq!(remaining_lines.len(), 3, "Expected 3 lines remaining, got: {:?}", remaining_lines);
    assert_eq!(remaining_lines[0], "line 17");
    assert_eq!(remaining_lines[1], "line 18");
    assert_eq!(remaining_lines[2], "line 19");

    // Single undo should restore all 17 deleted lines
    doc.undo();
    let after_undo = doc.buffer.to_string();
    let restored_line_count = after_undo.lines().count();
    assert_eq!(restored_line_count, 20, "Undo should restore all 20 lines");
}

#[test]
fn test_execute_delete_5_lines_up() {
    let mut doc = create_doc();

    // Create 10 lines
    for i in 0..10 {
        doc.buffer.insert_str(&format!("line {}\n", i)).unwrap();
    }

    // Position cursor on line 7
    doc.buffer.move_to_start();
    for _ in 0..7 {
        doc.buffer.move_down();
    }

    let current_line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
    assert_eq!(current_line, 7, "Should be on line 7");

    // Execute d5k (delete with motion Up, count 5)
    // This should delete from line 2 up to line 7 (6 lines total: current + 5 above)
    execute_command(
        Command::Delete(Motion::Up, 5),
        &mut doc,
        false,
        8,
        24,
        None,
    )
    .unwrap();

    let after_delete = doc.buffer.to_string();
    let remaining_lines: Vec<&str> = after_delete.lines().collect();

    // Should have lines 0, 1, 8, 9 remaining (4 lines)
    // Lines 2-7 should be deleted (6 lines)
    eprintln!("Remaining lines: {:?}", remaining_lines);
    assert_eq!(remaining_lines.len(), 4, "Expected 4 lines remaining, got: {:?}", remaining_lines);
    assert_eq!(remaining_lines[0], "line 0");
    assert_eq!(remaining_lines[1], "line 1");
    assert_eq!(remaining_lines[2], "line 8");
    assert_eq!(remaining_lines[3], "line 9");
}

#[test]
fn test_execute_delete_5_lines_up_cursor_mid_line() {
    let mut doc = create_doc();

    // Create 10 lines with longer content
    for i in 0..10 {
        doc.buffer.insert_str(&format!("line {} with some extra text\n", i)).unwrap();
    }

    // Position cursor on line 7, column 10 (middle of line)
    doc.buffer.move_to_start();
    for _ in 0..7 {
        doc.buffer.move_down();
    }
    // Move right to column 10
    for _ in 0..10 {
        doc.buffer.move_right();
    }

    let current_line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
    let line_start = doc.buffer.line_index.get_start(current_line).unwrap_or(0);
    let col = doc.buffer.cursor() - line_start;
    eprintln!("Starting at line {}, column {}", current_line, col);
    assert_eq!(current_line, 7, "Should be on line 7");
    assert_eq!(col, 10, "Should be at column 10");

    // Execute d5k (delete with motion Up, count 5)
    execute_command(
        Command::Delete(Motion::Up, 5),
        &mut doc,
        false,
        8,
        24,
        None,
    )
    .unwrap();

    let after_delete = doc.buffer.to_string();
    let remaining_lines: Vec<&str> = after_delete.lines().collect();

    eprintln!("Remaining lines: {:?}", remaining_lines);
    // Should have lines 0, 1, 8, 9 remaining (4 lines)
    assert_eq!(remaining_lines.len(), 4, "Expected 4 lines remaining, got: {:?}", remaining_lines);
    assert_eq!(remaining_lines[0], "line 0 with some extra text");
    assert_eq!(remaining_lines[1], "line 1 with some extra text");
    assert_eq!(remaining_lines[2], "line 8 with some extra text");
    assert_eq!(remaining_lines[3], "line 9 with some extra text");
}
