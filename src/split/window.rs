use crate::document::DocumentId;
use crate::viewport::Viewport;

pub type WindowId = u64;

pub struct Window {
    pub id: WindowId,
    pub document_id: DocumentId,
    pub viewport: Viewport,
    pub cursor_position: usize,
    pub frozen: bool,
}

impl Window {
    pub fn new(id: WindowId, document_id: DocumentId, rows: usize, cols: usize) -> Self {
        Window {
            id,
            document_id,
            viewport: Viewport::new(rows, cols),
            cursor_position: 0,
            frozen: false,
        }
    }
}
