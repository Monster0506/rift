use super::{Document, DocumentManager};

fn create_manager() -> DocumentManager {
    DocumentManager::new()
}

#[test]
fn test_iter_documents_visits_all() {
    let mut manager = create_manager();
    manager.insert(
        Document::new(crate::document::DocumentId::new(1)).unwrap(),
        crate::document::manager::Placement::Background,
    );
    manager.insert(
        Document::new(crate::document::DocumentId::new(2)).unwrap(),
        crate::document::manager::Placement::Background,
    );
    manager.insert(
        Document::new_directory(
            crate::document::DocumentId::new(3),
            std::path::PathBuf::from("/tmp"),
        )
        .unwrap(),
        crate::document::manager::Placement::Background,
    );

    let ids: Vec<crate::document::DocumentId> = manager.iter_documents().map(|d| d.id).collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&crate::document::DocumentId::new(1)));
    assert!(ids.contains(&crate::document::DocumentId::new(2)));
    assert!(ids.contains(&crate::document::DocumentId::new(3)));
}

#[test]
fn test_iter_documents_mut_allows_mutation() {
    let mut manager = create_manager();
    manager.insert(
        Document::new(crate::document::DocumentId::new(1)).unwrap(),
        crate::document::manager::Placement::Background,
    );
    for doc in manager.iter_documents_mut() {
        doc.set_read_only(true);
    }
    assert!(manager
        .get_document(crate::document::DocumentId::new(1))
        .unwrap()
        .is_read_only());
}

#[test]
fn test_manager_initial_state() {
    let manager = create_manager();
    assert_eq!(manager.tab_count(), 0);
    assert!(manager.get_document_id_at(0).is_none());
}

#[test]
fn test_add_document() {
    let mut manager = create_manager();
    let doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    manager.insert(doc, crate::document::manager::Placement::Background);

    assert_eq!(manager.tab_count(), 1);
    assert_eq!(
        manager.get_document_id_at(0),
        Some(crate::document::DocumentId::new(1))
    );
}

#[test]
fn test_remove_document() {
    let mut manager = create_manager();
    let doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    manager.insert(doc, crate::document::manager::Placement::Background);

    assert!(manager
        .remove_document(crate::document::DocumentId::new(1))
        .is_ok());
    assert_eq!(manager.tab_count(), 1); // Should still have 1 doc
    assert_ne!(
        manager.get_document_id_at(0),
        Some(crate::document::DocumentId::new(1))
    ); // But different ID
}

#[test]
fn test_remove_specific_document() {
    let mut manager = create_manager();
    let doc1 = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let doc2 = Document::new(crate::document::DocumentId::new(2)).unwrap();

    manager.insert(doc1, crate::document::manager::Placement::Background);
    manager.insert(doc2, crate::document::manager::Placement::Background);

    assert_eq!(manager.tab_count(), 2);
    assert!(manager
        .remove_document(crate::document::DocumentId::new(1))
        .is_ok());
    assert_eq!(manager.tab_count(), 1);
    assert_eq!(
        manager.get_document_id_at(0),
        Some(crate::document::DocumentId::new(2))
    );

    assert!(manager
        .remove_document(crate::document::DocumentId::new(2))
        .is_ok());
    assert_eq!(manager.tab_count(), 1);
    assert_ne!(
        manager.get_document_id_at(0),
        Some(crate::document::DocumentId::new(2))
    );
}

#[test]
fn neighboring_public_tabs_wrap_in_both_directions() {
    let mut manager = create_manager();
    let [first, second, third] = [1, 2, 3].map(crate::document::DocumentId::new);
    for id in [first, second, third] {
        manager.insert(
            Document::new(id).unwrap(),
            crate::document::manager::Placement::Background,
        );
    }

    assert_eq!(manager.neighbor_tab(third, false), Some(second));
    assert_eq!(manager.neighbor_tab(second, false), Some(first));
    assert_eq!(manager.neighbor_tab(first, false), Some(third));
    assert_eq!(manager.neighbor_tab(third, true), Some(first));
}

#[test]
fn neighboring_a_private_tab_returns_the_first_public_tab() {
    let mut manager = create_manager();
    let [first, second, private] = [1, 2, 3].map(crate::document::DocumentId::new);
    for id in [first, second] {
        manager.insert(
            Document::new(id).unwrap(),
            crate::document::manager::Placement::Background,
        );
    }
    manager.insert(
        Document::new(private).unwrap(),
        crate::document::manager::Placement::Private,
    );

    assert_eq!(manager.neighbor_tab(private, true), Some(first));
    assert_eq!(manager.neighbor_tab(private, false), Some(first));
    assert_eq!(manager.neighbor_tab(second, true), Some(first));
}
