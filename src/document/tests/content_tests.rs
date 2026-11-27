use super::*;
use crate::document::ops::{ContentKind, Edit, EditSource};

#[test]
fn a_read_only_edit_is_distinguishable_from_a_boundary_no_op() {
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    assert!(!doc.edit(Edit::DeleteBackward, EditSource::User).unwrap());
    doc.set_read_only(true);

    let error = doc
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap_err();

    assert_eq!(error.code, "READ_ONLY");
    assert!(doc.buffer().is_empty());
    assert!(!doc.is_dirty());
}

#[test]
fn an_edit_group_that_returns_an_error_does_not_absorb_the_next_edit() {
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let failed = doc.edit_group("Grouped edit", |doc| {
        doc.edit(Edit::type_text("first"), EditSource::User)?;
        doc.edit(Edit::delete(0..100), EditSource::User)
    });
    assert!(failed.is_err());
    doc.edit(Edit::type_text(" second"), EditSource::User)
        .unwrap();

    assert!(doc.undo());
    assert_eq!(doc.buffer().to_string(), "first");
    assert!(doc.undo());
    assert!(doc.buffer().is_empty());
}

#[test]
fn loading_content_advances_the_revision_and_resets_undo_state() {
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.edit(Edit::type_text("abc"), EditSource::User).unwrap();
    doc.edit(Edit::DeleteBackward, EditSource::User).unwrap();
    doc.begin_transaction("open");
    doc.edit(Edit::type_text("x"), EditSource::User).unwrap();
    let revision_before = doc.buffer.revision;
    let table = crate::buffer::rope::PieceTable::new(vec![crate::character::Character::from('z')]);
    let lines = crate::buffer::line_index::LineIndex::from_table_with_starts(table, vec![0]);
    doc.replace_content(lines, ContentKind::Load);

    assert!(doc.buffer.revision > revision_before);
    assert_eq!(doc.transaction_depth, 0);
    assert!(doc.annotation_undo_stack.is_empty());
    assert!(doc.annotation_redo_stack.is_empty());
    #[cfg(feature = "lsp")]
    assert!(doc.pending_lsp_edits.is_empty());
    assert_eq!(doc.buffer.to_string(), "z");
    assert!(doc
        .check_invariants(
            &BufferKindRegistry::with_builtins(),
            crate::invariants::InvariantTier::Standard
        )
        .is_empty());
}

#[cfg(feature = "lsp")]
#[test]
fn undo_and_redo_queue_lsp_changes_and_bump_the_content_version() {
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.edit(Edit::type_text("abc"), EditSource::User).unwrap();
    doc.discard_pending_lsp_changes();

    let version = doc.version();
    assert!(doc.undo());
    assert!(doc.version() > version);
    assert_eq!(doc.pending_lsp_edits.len(), 1);

    doc.discard_pending_lsp_changes();
    let version = doc.version();
    assert!(doc.redo());
    assert!(doc.version() > version);
    assert_eq!(doc.pending_lsp_edits.len(), 1);
}
