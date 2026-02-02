use super::*;

fn create_manager() -> DocumentManager {
    DocumentManager::new()
}

#[test]
fn test_manager_initial_state() {
    let manager = create_manager();
    assert_eq!(manager.tab_count(), 0);
    assert_eq!(manager.active_tab_index(), 0);
    assert!(manager.active_document().is_none());
    assert!(manager.active_document_id().is_none());
}

#[test]
fn test_add_document() {
    let mut manager = create_manager();
    let doc = Document::new(1).unwrap();
    manager.add_document(doc);

    assert_eq!(manager.tab_count(), 1);
    assert_eq!(manager.active_tab_index(), 0);
    assert_eq!(manager.active_document_id(), Some(1));
    assert!(manager.active_document().is_some());
}

#[test]
fn test_remove_document() {
    let mut manager = create_manager();
    let doc = Document::new(1).unwrap();
    manager.add_document(doc);

    assert!(manager.remove_document(1).is_ok());
    assert_eq!(manager.tab_count(), 1); // Should still have 1 doc
    assert_ne!(manager.active_document_id(), Some(1)); // But different ID
}

#[test]
fn test_remove_specific_document() {
    let mut manager = create_manager();
    let doc1 = Document::new(1).unwrap();
    let doc2 = Document::new(2).unwrap();

    manager.add_document(doc1);
    manager.add_document(doc2);

    assert_eq!(manager.tab_count(), 2);
    assert_eq!(manager.active_document_id(), Some(2)); // doc2 is active (latest added)

    // Remove doc1 (inactive)
    assert!(manager.remove_document(1).is_ok());
    assert_eq!(manager.tab_count(), 1);
    assert_eq!(manager.active_document_id(), Some(2)); // doc2 still active

    // Remove doc2 (active) - should create new empty one since it's the last one
    assert!(manager.remove_document(2).is_ok());
    assert_eq!(manager.tab_count(), 1);
    assert_ne!(manager.active_document_id(), Some(2));
}

#[test]
fn test_switching_tabs() {
    let mut manager = create_manager();
    let doc1 = Document::new(1).unwrap();
    let doc2 = Document::new(2).unwrap();
    let doc3 = Document::new(3).unwrap();

    manager.add_document(doc1);
    manager.add_document(doc2);
    manager.add_document(doc3);

    // Initial: [1, 2, 3], current=2 (index)
    assert_eq!(manager.active_document_id(), Some(3));

    manager.switch_prev_tab();
    assert_eq!(manager.active_document_id(), Some(2));

    manager.switch_prev_tab();
    assert_eq!(manager.active_document_id(), Some(1));

    manager.switch_prev_tab(); // Wrap around
    assert_eq!(manager.active_document_id(), Some(3));

    manager.switch_next_tab(); // Wrap around
    assert_eq!(manager.active_document_id(), Some(1));
}

#[test]
fn test_open_existing_file_switches_tab() {
    let _manager = create_manager();
}

#[test]
fn test_undo_binary_data() {
    let mut doc = Document::new(1).unwrap();
    // Insert binary byte directly into buffer to simulate file load (invalid UTF-8)
    // 0xFF is not a valid UTF-8 byte
    let binary_char = crate::character::Character::Byte(0xFF);
    doc.buffer.insert_character(binary_char).unwrap();

    assert_eq!(doc.buffer.len(), 1);
    assert_eq!(doc.buffer.char_at(0), Some(binary_char));

    // Delete it using Document API (which should record it in history)
    doc.delete_range(0, 1).unwrap();
    assert_eq!(doc.buffer.len(), 0);

    // Undo
    doc.undo();

    // Verify the ORIGINAL byte is restored, not a replacement character
    assert_eq!(doc.buffer.len(), 1);
    assert_eq!(doc.buffer.char_at(0), Some(binary_char));
}

#[test]
fn test_get_changed_line_for_seq() {
    let mut doc = Document::new(1).unwrap();

    // Initial state has seq=0, no operations
    assert_eq!(doc.get_changed_line_for_seq(0), None);

    // Insert text at line 0
    doc.insert_str("hello\n").unwrap();
    let seq1 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq1), Some(0));

    // Insert more text (still at line 0 because cursor is at end of line 1)
    doc.insert_str("world\n").unwrap();
    let seq2 = doc.history.current_seq();
    // The insertion happened at line 1 (after the first newline)
    assert_eq!(doc.get_changed_line_for_seq(seq2), Some(1));

    // Insert text at a specific position (move cursor to line 2)
    doc.insert_str("line3").unwrap();
    let seq3 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq3), Some(2));

    // Test with delete operation
    doc.buffer.set_cursor(0).unwrap(); // Go to start
    doc.delete_forward(); // Delete 'h'
    let seq4 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq4), Some(0));

    // Test invalid seq
    assert_eq!(doc.get_changed_line_for_seq(9999), None);
}
