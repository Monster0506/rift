use crate::document::DocumentId;
use crate::viewport::Viewport;

pub type WindowId = u64;

pub struct Window {
    pub id: WindowId,
    pub document_id: DocumentId,
    pub viewport: Viewport,
    pub cursor_position: usize,
    /// When Some, this window is frozen and `document_id` points to a private copy.
    /// Stores the original shared document ID for re-attaching on nofreeze.
    pub original_document_id: Option<DocumentId>,
}

impl Window {
    pub fn new(id: WindowId, document_id: DocumentId, rows: usize, cols: usize) -> Self {
        Window {
            id,
            document_id,
            viewport: Viewport::new(rows, cols),
            cursor_position: 0,
            original_document_id: None,
        }
    }

    pub fn is_frozen(&self) -> bool {
        self.original_document_id.is_some()
    }

    pub fn canonical_document_id(&self) -> DocumentId {
        self.original_document_id.unwrap_or(self.document_id)
    }
}
