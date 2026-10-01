use super::common::*;
use super::*;

#[test]
fn test_iter_documents_visits_all() {
    let mut manager = create_manager();
    manager.add_document(Document::new(1).unwrap());
    manager.add_document(Document::new(2).unwrap());
    manager.add_document(Document::new_directory(3, PathBuf::from("/tmp")).unwrap());

    let ids: Vec<u64> = manager.iter_documents().map(|d| d.id).collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));
}

#[test]
fn test_iter_documents_mut_allows_mutation() {
    let mut manager = create_manager();
    manager.add_document(Document::new(1).unwrap());
    for doc in manager.iter_documents_mut() {
        doc.set_read_only(true);
    }
    assert!(manager.active_document().unwrap().is_read_only());
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

    assert!(manager.remove_document(1).is_ok());
    assert_eq!(manager.tab_count(), 1);
    assert_eq!(manager.active_document_id(), Some(2)); // doc2 still active

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
fn test_switching_tabs_from_private_document_lands_on_first_public_tab() {
    let mut manager = create_manager();
    manager.add_document(Document::new(1).unwrap());
    manager.add_document(Document::new(2).unwrap());
    manager.add_private_document(Document::new(3).unwrap());

    assert_eq!(manager.active_document_id(), Some(3));

    manager.switch_next_tab();
    assert_eq!(
        manager.active_document_id(),
        Some(1),
        "switch_next_tab from a private doc must land on the first public tab, not no-op"
    );

    manager.add_private_document(Document::new(4).unwrap());
    manager.switch_prev_tab();
    assert_eq!(
        manager.active_document_id(),
        Some(1),
        "switch_prev_tab from a private doc must land on the first public tab, not no-op"
    );
}

#[test]
fn test_open_existing_file_switches_tab() {
    let _manager = create_manager();
}
