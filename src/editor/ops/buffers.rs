use super::super::Editor;
use crate::document::manager::{NewDocument, Placement};
use crate::document::DocumentId;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn open_buffer(
        &mut self,
        new: impl Into<NewDocument>,
        placement: Placement,
    ) -> DocumentId {
        let handle = self.document_manager.insert(new, placement);
        if let Some(kind) = self
            .document_manager
            .get_document(handle.doc_id())
            .map(crate::document::Document::buffer_kind_id)
        {
            self.buffer_kinds.increment_open_count(kind);
        }
        handle.doc_id()
    }

    pub(crate) fn open_placeholder(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<DocumentId, crate::error::RiftError> {
        let mut doc = crate::document::Document::new(self.document_manager.next_id())?;
        doc.set_path(path.as_ref());
        Ok(self.open_buffer(doc, Placement::Focused))
    }
}
