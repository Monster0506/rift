use super::*;

#[test]
fn open_file_creates_empty_document_for_nonexistent_path_in_existing_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brand_new.txt");
    let path_str = path.to_string_lossy().into_owned();

    let mut mgr = DocumentManager::new();
    mgr.open_file(Some(path_str), false).unwrap();

    let doc = mgr.active_document().unwrap();
    assert_eq!(doc.path(), Some(path.as_path()));
    assert_eq!(doc.buffer.len(), 0);
    assert!(!path.exists(), "opening must not touch disk");
}

#[test]
fn open_file_rejects_path_whose_parent_directory_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no_such_subdir").join("file.txt");
    let path_str = path.to_string_lossy().into_owned();

    let mut mgr = DocumentManager::new();
    let err = mgr.open_file(Some(path_str), false).unwrap_err();

    assert_eq!(err.code, crate::constants::errors::PARENT_DIR_MISSING);
    assert!(mgr.active_document().is_none());
}

#[test]
fn open_file_rejects_existing_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path_str = dir.path().to_string_lossy().into_owned();

    let mut mgr = DocumentManager::new();
    let err = mgr.open_file(Some(path_str), false).unwrap_err();

    assert_eq!(err.code, crate::constants::errors::NOT_A_FILE);
}

#[test]
fn prepare_removal_normal_rejects_dirty_without_side_effects() {
    let mut mgr = DocumentManager::new();
    let mut doc = Document::new(mgr.next_id()).unwrap();
    doc.insert_str("unsaved changes").unwrap();
    let id = doc.id;
    mgr.add_document(doc);

    assert!(mgr.documents.get(&id).unwrap().is_dirty());
    let err = mgr.prepare_removal(id, RemovalIntent::Normal).unwrap_err();
    assert_eq!(err.code, crate::constants::errors::UNSAVED_CHANGES);

    // Verify side-effect-free: document still in manager and tab order intact
    assert!(mgr.documents.contains_key(&id));
    assert_eq!(mgr.tab_order.len(), 1);
    assert_eq!(mgr.active_document_id(), Some(id));
}

#[test]
fn prepare_removal_force_accepts_dirty() {
    let mut mgr = DocumentManager::new();
    let mut doc = Document::new(mgr.next_id()).unwrap();
    doc.buffer.insert_str("unsaved changes").unwrap();
    let id = doc.id;
    mgr.add_document(doc);

    let plan = mgr.prepare_removal(id, RemovalIntent::Force).unwrap();
    assert_eq!(plan.id(), id);
    assert_eq!(plan.intent(), RemovalIntent::Force);
    assert!(plan.has_replacement());

    let removed = mgr.commit_removal(plan).unwrap();
    assert_eq!(removed.id, id);
    assert_eq!(mgr.tab_order.len(), 1);
    assert_ne!(mgr.active_document_id(), Some(id));
}

#[test]
fn prepare_removal_last_tab_prepares_replacement_before_commit() {
    let mut mgr = DocumentManager::new();
    let doc = Document::new(mgr.next_id()).unwrap();
    let id = doc.id;
    mgr.add_document(doc);

    let plan = mgr.prepare_removal(id, RemovalIntent::Normal).unwrap();
    assert!(plan.replacement.is_some());
    let repl_id = plan.replacement.as_ref().unwrap().id;
    assert_ne!(repl_id, id);

    let removed = mgr.commit_removal(plan).unwrap();
    assert_eq!(removed.id, id);
    assert_eq!(mgr.tab_order.len(), 1);
    assert_eq!(mgr.active_document_id(), Some(repl_id));
}

#[test]
fn creation_reservation_and_draft_commit() {
    let mut mgr = DocumentManager::new();
    let res = mgr.reserve_creation();
    assert_eq!(res.id(), 1);
    assert_eq!(res.handle().doc_id(), 1);

    let doc = Document::new(res.id()).unwrap();
    let draft = DocumentDraft::new(res, doc).unwrap();

    assert!(mgr.get_document(res.id()).is_none());

    let handle = mgr.commit_draft_active(draft);
    assert_eq!(handle, res.handle());
    assert_eq!(mgr.active_document_id(), Some(res.id()));
    assert_eq!(mgr.active_document_handle(), Some(handle));
    assert_eq!(mgr.active_document().map(Document::handle), Some(handle));
}

#[test]
fn draft_rejects_descriptor_state_mismatch() {
    let mut manager = DocumentManager::new();
    let reservation = manager.reserve_creation();
    let mut document = Document::new(reservation.id()).unwrap();
    document.kind = crate::document::BufferKind::directory();

    let error = DocumentDraft::new(reservation, document)
        .err()
        .expect("mismatched descriptor state must be rejected");
    assert_eq!(error.kind, ErrorType::Internal);
    assert!(manager.get_document(reservation.id()).is_none());
}
