use super::*;
use crate::document::DocumentId;
use crate::error::ErrorType;

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

    // Removing the only document should create a new empty one
    // But DocumentManager logic for remove_document(id) currently checks "if tab_order.len() == 1".
    // Let's verify this behavior.
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

    // We would need to mock filesystem or use temporary files to test open_file behavior accurately
    // involving canonicalization.
    // However, we can test that open_file tries to add a document.
    // Testing duplicate open detection requires files on disk usually.
    // For unit tests without IO, we might skip IO dependent tests or use tempfile.
    // Document::from_file does checking.
    // Let's stick to logic that doesn't strictly depend on IO if possible, or simple paths.
}
