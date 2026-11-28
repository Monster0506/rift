use super::Editor;
use crate::document::DocumentId;
use crate::history::{EditSeq, UndoTree};
use crate::term::TerminalBackend;
use std::path::Path;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn restore_persisted_undo(
        &mut self,
        doc_id: DocumentId,
        content_hash: crate::history::persist::Sha256Digest,
    ) {
        if !self.state.settings.undo().persistent {
            return;
        }
        let undo_dir =
            crate::history::persist::resolve_undo_dir(self.state.settings.undo().dir.as_deref());
        let Some(doc) = self.document_manager.get_document_mut(doc_id) else {
            return;
        };
        let Some(path) = doc.path() else {
            return;
        };
        if let Some(tree) = crate::history::persist::load(&undo_dir, path, content_hash) {
            doc.history = tree;
        }
    }

    pub(super) fn persist_undo_after_save(
        &self,
        path: &Path,
        tree: &UndoTree,
        saved_seq: EditSeq,
        content_hash: crate::history::persist::Sha256Digest,
    ) {
        if !self.state.settings.undo().persistent {
            return;
        }
        let undo_dir =
            crate::history::persist::resolve_undo_dir(self.state.settings.undo().dir.as_deref());
        let _ = crate::history::persist::save_at(&undo_dir, path, tree, saved_seq, content_hash);
    }
}

#[cfg(test)]
#[path = "undo_persist_tests.rs"]
mod undo_persist_tests;
