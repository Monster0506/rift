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

#[test]
fn public_tab_numbering_and_removal_skip_private_entries() {
    use crate::document::{
        manager::{Placement, RemovalIntent},
        DocumentId,
    };

    let mut manager = create_manager();
    let [first, private, last, trailing_private] = [1, 2, 3, 4].map(DocumentId::new);
    for (id, placement) in [
        (first, Placement::Background),
        (private, Placement::Private),
        (last, Placement::Background),
        (trailing_private, Placement::Private),
    ] {
        manager.insert(Document::new(id).unwrap(), placement);
    }

    assert_eq!(manager.tab_count(), 2);
    assert_eq!(manager.get_document_id_at(1), Some(private));
    assert_eq!(manager.tab_index(last), Some(2));
    assert_eq!(manager.public_tab_at(0), Some(first));
    assert_eq!(manager.public_tab_at(1), Some(last));
    assert_eq!(manager.public_tab_at(2), None);
    assert_eq!(
        manager
            .get_buffer_list(private)
            .into_iter()
            .map(|info| (info.index, info.id, info.is_current))
            .collect::<Vec<_>>(),
        [(0, first, false), (1, last, false)]
    );
    assert_eq!(
        manager
            .prepare_removal(first, RemovalIntent::Normal)
            .unwrap()
            .replacement,
        Some(last)
    );
    assert_eq!(
        manager
            .prepare_removal(last, RemovalIntent::Normal)
            .unwrap()
            .replacement,
        Some(first)
    );

    manager.remove_document(last).unwrap();
    assert!(manager
        .prepare_removal(first, RemovalIntent::Normal)
        .unwrap()
        .needs_replacement());
}

#[test]
fn open_file_lookup_ignores_a_private_preview_of_the_same_path() {
    use crate::document::manager::Placement;

    let mut manager = create_manager();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("preview.txt");
    let private = manager.next_id();
    let mut preview = Document::new(private).unwrap();
    preview.set_path(&path);
    manager.insert(preview, Placement::Private);
    assert_eq!(manager.find_open_document_id(&path), None);

    let public = manager.next_id();
    let mut file = Document::new(public).unwrap();
    file.set_path(&path);
    manager.insert(file, Placement::Background);
    assert_eq!(manager.find_open_document_id(&path), Some(public));
}
