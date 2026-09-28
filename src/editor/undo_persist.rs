use super::Editor;
use crate::document::DocumentId;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn restore_persisted_undo(
        &mut self,
        doc_id: DocumentId,
        content_hash: crate::history::persist::Sha256Digest,
    ) {
        if !self.state.settings.persistent_undo {
            return;
        }
        let undo_dir =
            crate::history::persist::resolve_undo_dir(self.state.settings.undo_dir.as_deref());
        let Some(doc) = self.document_manager.get_document_mut(doc_id) else {
            return;
        };
        let Some(path) = doc.path().map(std::path::Path::to_path_buf) else {
            return;
        };
        if let Some(tree) = crate::history::persist::load(&undo_dir, &path, content_hash) {
            doc.history = tree;
        }
    }

    pub(super) fn persist_undo_after_save(
        &mut self,
        doc_id: DocumentId,
        content_hash: crate::history::persist::Sha256Digest,
    ) {
        if !self.state.settings.persistent_undo {
            return;
        }
        let undo_dir =
            crate::history::persist::resolve_undo_dir(self.state.settings.undo_dir.as_deref());
        let Some(doc) = self.document_manager.get_document(doc_id) else {
            return;
        };
        let Some(path) = doc.path().map(std::path::Path::to_path_buf) else {
            return;
        };
        let _ = crate::history::persist::save(&undo_dir, &path, &doc.history, content_hash);
    }
}

#[cfg(test)]
#[path = "undo_persist_tests.rs"]
mod undo_persist_tests;
