use super::*;
use crate::document::ops::{Edit, EditSource};

#[test]
fn test_remove_directory_buffer_without_dirty_check() {
    let mut manager = DocumentManager::new();
    let mut doc = Document::new_directory(
        crate::document::DocumentId::new(1),
        std::path::PathBuf::from("/tmp"),
    )
    .unwrap();
    doc.replace_content("some edit", crate::document::ops::ContentKind::Render);
    manager.insert(doc, crate::document::manager::Placement::Background);

    assert!(manager
        .remove_document(crate::document::DocumentId::new(1))
        .is_ok());
}

#[test]
fn test_remove_undotree_buffer_without_dirty_check() {
    let mut manager = DocumentManager::new();
    let doc = Document::new_undotree(
        crate::document::DocumentId::new(2),
        crate::document::DocumentId::new(1),
    )
    .unwrap();
    manager.insert(doc, crate::document::manager::Placement::Background);
    assert!(manager
        .remove_document(crate::document::DocumentId::new(2))
        .is_ok());
}

#[test]
fn both_document_allocators_skip_claimed_plugin_ids_across_refills() {
    let mut manager = crate::document::manager::DocumentManager::new();
    let mut ids = manager.refill_plugin_ids(Vec::new());
    let claimed = ids.pop().unwrap();
    let next = manager.next_id();
    assert!(!ids.contains(&next.get()) && claimed != next.get());
    let editor_reservation = manager.reserve_creation();
    assert_eq!(editor_reservation.id(), next);
    let replacement_pool = manager.refill_plugin_ids(ids);
    assert!(!replacement_pool.contains(&claimed));
    let creation = manager
        .claim_plugin_creation(DocumentId::new(claimed))
        .unwrap();
    assert_eq!(creation.id().get(), claimed);
    assert!(manager
        .claim_plugin_creation(DocumentId::new(claimed))
        .is_none());
}

#[test]
fn open_path_creates_empty_document_for_nonexistent_path_in_existing_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brand_new.txt");
    let path_str = path.to_string_lossy().into_owned();

    let doc = Document::open_path(DocumentId::new(1), &path_str).unwrap();

    assert_eq!(doc.path(), Some(path.as_path()));
    assert_eq!(doc.buffer().len(), 0);
    assert!(!path.exists(), "opening must not touch disk");
}

#[test]
fn open_path_rejects_path_whose_parent_directory_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no_such_subdir").join("file.txt");
    let path_str = path.to_string_lossy().into_owned();

    let Err(err) = Document::open_path(DocumentId::new(1), &path_str) else {
        panic!("a missing parent directory must be rejected");
    };

    assert_eq!(err.code, crate::constants::errors::PARENT_DIR_MISSING);
}

#[test]
fn open_path_rejects_existing_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path_str = dir.path().to_string_lossy().into_owned();

    let Err(err) = Document::open_path(DocumentId::new(1), &path_str) else {
        panic!("a directory must be rejected");
    };

    assert_eq!(err.code, crate::constants::errors::NOT_A_FILE);
}

#[test]
fn prepare_removal_normal_rejects_dirty_without_side_effects() {
    let mut mgr = DocumentManager::new();
    let mut doc = Document::new(mgr.next_id()).unwrap();
    doc.edit(Edit::type_text("unsaved changes"), EditSource::User)
        .unwrap();
    let id = doc.id;
    mgr.insert(doc, crate::document::manager::Placement::Background);

    assert!(mgr.documents.get(&id).unwrap().is_dirty());
    let err = mgr.prepare_removal(id, RemovalIntent::Normal).unwrap_err();
    assert_eq!(err.code, crate::constants::errors::UNSAVED_CHANGES);

    // Verify side-effect-free: document still in manager and tab order intact
    assert!(mgr.documents.contains_key(&id));
    assert_eq!(mgr.tab_order.len(), 1);
    assert_eq!(mgr.get_document_id_at(0), Some(id));
}

#[test]
fn prepare_removal_force_accepts_dirty() {
    let mut mgr = DocumentManager::new();
    let mut doc = Document::new(mgr.next_id()).unwrap();
    doc.edit(Edit::type_text("unsaved changes"), EditSource::User)
        .unwrap();
    let id = doc.id;
    mgr.insert(doc, crate::document::manager::Placement::Background);

    let plan = mgr.prepare_removal(id, RemovalIntent::Force).unwrap();
    assert_eq!(plan.id(), id);
    assert!(plan.needs_replacement());

    let removed = mgr.commit_removal(plan).unwrap();
    assert_eq!(removed.id, id);
    assert!(mgr.tab_order.is_empty());
}

#[test]
fn removing_the_last_tab_leaves_a_fresh_replacement() {
    let mut mgr = DocumentManager::new();
    let doc = Document::new(mgr.next_id()).unwrap();
    let id = doc.id;
    mgr.insert(doc, crate::document::manager::Placement::Background);

    assert!(mgr
        .prepare_removal(id, RemovalIntent::Normal)
        .unwrap()
        .needs_replacement());
    mgr.remove_document(id).unwrap();

    assert_eq!(mgr.tab_order.len(), 1);
    assert!(mgr.get_document_id_at(0).is_some_and(|active| active != id));
}

#[test]
fn creation_reservation_and_draft_commit() {
    let mut mgr = DocumentManager::new();
    let res = mgr.reserve_creation();
    assert_eq!(res.id(), crate::document::DocumentId::new(1));
    assert_eq!(res.handle().doc_id(), crate::document::DocumentId::new(1));

    let doc = Document::new(res.id()).unwrap();
    let draft = DocumentDraft::new(res, doc).unwrap();

    assert!(mgr.get_document(res.id()).is_none());

    let handle = mgr.insert(draft, crate::document::manager::Placement::Background);
    assert_eq!(handle, res.handle());
    assert_eq!(mgr.get_document_id_at(0), Some(res.id()));
    assert_eq!(
        mgr.get_document_by_handle(handle).map(Document::handle),
        Some(handle)
    );
}

#[test]
fn draft_rejects_descriptor_state_mismatch() {
    let mut manager = DocumentManager::new();
    let reservation = manager.reserve_creation();
    let mut document = Document::new(reservation.id()).unwrap();
    document.kind = Document::new_directory(reservation.id(), std::path::PathBuf::from("/tmp"))
        .unwrap()
        .kind;

    let error = DocumentDraft::new(reservation, document)
        .err()
        .expect("mismatched descriptor state must be rejected");
    assert_eq!(error.kind, ErrorType::Internal);
    assert!(manager.get_document(reservation.id()).is_none());
}
