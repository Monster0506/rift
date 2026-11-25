use super::*;
use crate::document::ops::{ContentKind, Edit, EditSource};

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
    assert!(doc.pending_lsp_edits.is_empty());
    assert_eq!(doc.buffer.to_string(), "z");
    assert!(doc
        .check_invariants(
            &BufferKindRegistry::with_builtins(),
            crate::invariants::InvariantTier::Standard
        )
        .is_empty());
}

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
