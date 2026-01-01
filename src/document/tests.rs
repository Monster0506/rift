//! Tests for document module

use super::*;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_document_new() {
    let doc = Document::new(1).unwrap();
    assert_eq!(doc.id, 1);
    assert_eq!(doc.revision, 0);
    assert_eq!(doc.last_saved_revision, 0);
    assert!(!doc.is_dirty());
    assert!(doc.is_empty());
    assert!(!doc.has_path());
    assert_eq!(doc.display_name(), "[No Name]");
}

#[test]
fn test_document_mark_dirty() {
    let mut doc = Document::new(1).unwrap();
    assert!(!doc.is_dirty());

    doc.mark_dirty();
    assert!(doc.is_dirty());
    assert_eq!(doc.revision, 1);

    doc.mark_dirty();
    assert_eq!(doc.revision, 2);
}

#[test]
fn test_document_from_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    // Create test file
    let mut file = fs::File::create(&file_path).unwrap();
    file.write_all(b"Hello, world!").unwrap();
    drop(file);

    // Load document
    let doc = Document::from_file(1, &file_path).unwrap();
    assert_eq!(doc.id, 1);
    assert!(!doc.is_dirty());
    assert!(!doc.is_empty());
    assert!(doc.has_path());
    assert_eq!(doc.display_name(), "test.txt");
    assert_eq!(doc.path(), Some(file_path.as_path()));

    // Verify buffer contents
    assert_eq!(doc.buffer.to_string(), "Hello, world!");
}

#[test]
fn test_document_has_path() {
    let mut doc = Document::new(1).unwrap();
    assert!(!doc.has_path());

    doc.set_path("test.txt");
    assert!(doc.has_path());
}

#[test]
fn test_document_save() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    // Create document and add content
    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("Hello, world!").unwrap();
    doc.mark_dirty();
    assert!(doc.is_dirty());

    // Set path and save
    doc.set_path(&file_path);
    doc.save().unwrap();

    // Verify dirty flag cleared
    assert!(!doc.is_dirty());

    // Verify file contents
    let contents = fs::read_to_string(&file_path).unwrap();
    assert_eq!(contents, "Hello, world!");
}

#[test]
fn test_document_save_clears_dirty() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("Test content").unwrap();
    doc.set_path(&file_path);

    doc.mark_dirty();
    assert!(doc.is_dirty());

    doc.save().unwrap();
    assert!(!doc.is_dirty());
}

#[test]
fn test_document_save_without_path() {
    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("Test").unwrap();

    let result = doc.save();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind, ErrorType::Io);
    assert_eq!(err.code, "NO_PATH");
}

#[test]
fn test_document_save_as() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("new.txt");

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("Test data").unwrap();
    doc.mark_dirty();

    // Save to new path
    doc.save_as(&file_path).unwrap();

    // Verify path updated
    assert!(doc.has_path());
    assert_eq!(doc.path(), Some(file_path.as_path()));
    assert!(!doc.is_dirty());

    // Verify file contents
    let contents = fs::read_to_string(&file_path).unwrap();
    assert_eq!(contents, "Test data");
}

#[test]
fn test_document_display_name() {
    let mut doc = Document::new(1).unwrap();
    assert_eq!(doc.display_name(), "[No Name]");

    doc.set_path("/path/to/file.txt");
    assert_eq!(doc.display_name(), "file.txt");

    doc.set_path("relative/path/test.rs");
    assert_eq!(doc.display_name(), "test.rs");
}

#[test]
fn test_document_is_empty() {
    let mut doc = Document::new(1).unwrap();
    assert!(doc.is_empty());

    doc.buffer.insert_char('a').unwrap();
    assert!(!doc.is_empty());
}

#[test]
fn test_document_reload_from_disk() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    // Create initial file
    fs::write(&file_path, b"Initial content").unwrap();

    // Load document
    let mut doc = Document::from_file(1, &file_path).unwrap();
    assert_eq!(doc.buffer.to_string(), "Initial content");

    // Modify in memory
    doc.buffer.move_to_end();
    doc.buffer.insert_str(" modified").unwrap();
    doc.mark_dirty();
    assert_eq!(doc.buffer.to_string(), "Initial content modified");

    // Update file on disk
    fs::write(&file_path, b"Updated content").unwrap();

    // Reload from disk
    doc.reload_from_disk().unwrap();
    assert_eq!(doc.buffer.to_string(), "Updated content");
    assert!(!doc.is_dirty());
}

#[test]
fn test_document_atomic_write() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("atomic.txt");

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("Atomic write test").unwrap();
    doc.set_path(&file_path);

    // Save should be atomic
    doc.save().unwrap();

    // Temp file should not exist
    let temp_path = temp_dir.path().join(".atomic.txt.tmp");
    assert!(!temp_path.exists());

    // Final file should exist with correct content
    assert!(file_path.exists());
    let contents = fs::read_to_string(&file_path).unwrap();
    assert_eq!(contents, "Atomic write test");
}

#[test]
fn test_document_revision_tracking() {
    let mut doc = Document::new(1).unwrap();
    assert_eq!(doc.revision, 0);
    assert_eq!(doc.last_saved_revision, 0);

    // Multiple edits increment revision
    doc.mark_dirty();
    assert_eq!(doc.revision, 1);

    doc.mark_dirty();
    assert_eq!(doc.revision, 2);

    // Save updates last_saved_revision
    doc.set_path("test.txt");
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.txt");
    doc.save_as(&path).unwrap();

    assert_eq!(doc.revision, 2);
    assert_eq!(doc.last_saved_revision, 2);
    assert!(!doc.is_dirty());

    // New edit makes dirty again
    doc.mark_dirty();
    assert_eq!(doc.revision, 3);
    assert!(doc.is_dirty());
}

#[test]
fn test_document_from_file_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("crlf.txt");

    // Create test file with CRLF
    let mut file = fs::File::create(&file_path).unwrap();
    file.write_all(b"line1\r\nline2\r\n").unwrap();
    drop(file);

    // Load document
    let doc = Document::from_file(1, &file_path).unwrap();

    // In CRLF files, \r should now be normalized (removed)
    assert_eq!(doc.options.line_ending, LineEnding::CRLF);

    // Line 0 should be "line1" (no trailing \r)
    let line0 = doc.buffer.get_line_bytes(0);
    assert_eq!(line0, b"line1");

    // Line 1 should be "line2" (no trailing \r)
    let line1 = doc.buffer.get_line_bytes(1);
    assert_eq!(line1, b"line2");

    assert_eq!(doc.buffer.get_total_lines(), 3);
}

#[test]
fn test_document_save_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("crlf_save.txt");

    let mut doc = Document::new(1).unwrap();
    doc.options.line_ending = LineEnding::CRLF;
    doc.buffer.insert_str("line1\nline2\n").unwrap();
    doc.set_path(&file_path);

    doc.save().unwrap();

    // Verify file contents on disk have CRLF
    let bytes = fs::read(&file_path).unwrap();
    assert_eq!(bytes, b"line1\r\nline2\r\n");
}

// =============================================================================
// Undo/Redo Integration Tests
// =============================================================================

#[test]
fn test_document_undo_insert_char() {
    let mut doc = Document::new(1).unwrap();

    // Insert characters
    doc.insert_char('a').unwrap();
    doc.insert_char('b').unwrap();
    doc.insert_char('c').unwrap();

    assert_eq!(doc.buffer.to_string(), "abc");

    // Undo last char
    assert!(doc.can_undo());
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "ab");

    // Undo another
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "a");

    // Undo again
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");

    // No more undos
    assert!(!doc.can_undo());
}

#[test]
fn test_document_redo_insert_char() {
    let mut doc = Document::new(1).unwrap();

    doc.insert_char('x').unwrap();
    doc.insert_char('y').unwrap();

    assert_eq!(doc.buffer.to_string(), "xy");

    // Undo both
    doc.undo();
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");

    // Redo first
    assert!(doc.can_redo());
    doc.redo();
    assert_eq!(doc.buffer.to_string(), "x");

    // Redo second
    doc.redo();
    assert_eq!(doc.buffer.to_string(), "xy");

    // No more redos
    assert!(!doc.can_redo());
}

#[test]
fn test_document_undo_delete_backward() {
    let mut doc = Document::new(1).unwrap();

    doc.insert_str("hello").unwrap();
    assert_eq!(doc.buffer.to_string(), "hello");

    // Delete 'o'
    doc.delete_backward();
    assert_eq!(doc.buffer.to_string(), "hell");

    // Undo delete - should restore 'o'
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "hello");
}

#[test]
fn test_document_undo_delete_forward() {
    let mut doc = Document::new(1).unwrap();

    doc.insert_str("world").unwrap();
    doc.buffer.move_to_start();
    assert_eq!(doc.buffer.to_string(), "world");

    // Delete 'w'
    doc.delete_forward();
    assert_eq!(doc.buffer.to_string(), "orld");

    // Undo delete - should restore 'w'
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "world");
}

#[test]
fn test_document_undo_insert_str() {
    let mut doc = Document::new(1).unwrap();

    doc.insert_str("Hello, World!").unwrap();
    assert_eq!(doc.buffer.to_string(), "Hello, World!");

    // Undo the entire string insert
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");
}

#[test]
fn test_document_transaction_grouping() {
    let mut doc = Document::new(1).unwrap();

    // Start a transaction
    doc.begin_transaction("Test insert");

    doc.insert_char('a').unwrap();
    doc.insert_char('b').unwrap();
    doc.insert_char('c').unwrap();

    // Commit the transaction
    doc.commit_transaction();

    assert_eq!(doc.buffer.to_string(), "abc");

    // One undo should remove all three characters (grouped transaction)
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");

    // One redo should restore all
    doc.redo();
    assert_eq!(doc.buffer.to_string(), "abc");
}

#[test]
fn test_document_undo_redo_cursor_position() {
    let mut doc = Document::new(1).unwrap();

    doc.insert_str("abc").unwrap();
    assert_eq!(doc.buffer.cursor(), 3);

    doc.undo();
    // Cursor position after undo - should be at start since content is gone
    assert_eq!(doc.buffer.to_string(), "");
}

#[test]
fn test_document_multiple_undo_redo_cycles() {
    let mut doc = Document::new(1).unwrap();

    // Insert, undo, redo multiple times
    doc.insert_char('X').unwrap();
    assert_eq!(doc.buffer.to_string(), "X");

    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");

    doc.redo();
    assert_eq!(doc.buffer.to_string(), "X");

    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");

    doc.redo();
    assert_eq!(doc.buffer.to_string(), "X");

    // Add more text
    doc.insert_char('Y').unwrap();
    assert_eq!(doc.buffer.to_string(), "XY");

    // Undo both
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "X");

    doc.undo();
    assert_eq!(doc.buffer.to_string(), "");
}

#[test]
fn test_document_undo_creates_branch() {
    let mut doc = Document::new(1).unwrap();

    doc.insert_char('A').unwrap();
    doc.insert_char('B').unwrap();

    // Undo B
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "A");

    // Insert C (creates branch)
    doc.insert_char('C').unwrap();
    assert_eq!(doc.buffer.to_string(), "AC");

    // Undo C
    doc.undo();
    assert_eq!(doc.buffer.to_string(), "A");

    // Can redo C (last visited branch)
    assert!(doc.can_redo());
    doc.redo();
    assert_eq!(doc.buffer.to_string(), "AC");
}

#[test]
fn test_document_can_undo_false_initially() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn test_document_can_redo_false_initially() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.can_redo());
}

#[test]
fn test_document_undo_marks_dirty() {
    let mut doc = Document::new(1).unwrap();

    doc.insert_char('x').unwrap();

    // Clear dirty flag (simulate save)
    doc.last_saved_revision = doc.revision;
    assert!(!doc.is_dirty());

    // Undo should mark dirty
    doc.undo();
    assert!(doc.is_dirty());
}
