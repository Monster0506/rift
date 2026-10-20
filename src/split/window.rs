use crate::document::DocumentId;
use crate::viewport::Viewport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(u64);

impl WindowId {
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct Window {
    pub id: WindowId,
    pub document_id: DocumentId,
    pub viewport: Viewport,
    pub cursor_position: usize,
}

impl Window {
    #[must_use]
    pub const fn new(id: WindowId, document_id: DocumentId, rows: usize, cols: usize) -> Self {
        Self {
            id,
            document_id,
            viewport: Viewport::new(rows, cols),
            cursor_position: 0,
        }
    }
}
