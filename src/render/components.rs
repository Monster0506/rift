use crate::layer::LayerPriority;
use crate::render::{CommandDrawState, ContentDrawState, NotificationDrawState, StatusDrawState};

/// Position and size of a visual element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub row: usize,
    pub col: usize,
    pub height: usize,
    pub width: usize,
}

impl Rect {
    pub fn new(row: usize, col: usize, height: usize, width: usize) -> Self {
        Self {
            row,
            col,
            height,
            width,
        }
    }
}

/// The visual content to be rendered
#[derive(Debug, Clone, PartialEq)]
pub enum Renderable {
    TextBuffer(ContentDrawState),
    StatusBar(StatusDrawState),
    Window(CommandDrawState),
    Notification(NotificationDrawState),
    RefToModal,
}

/// A component that defines the Z-ordering
pub type Layer = LayerPriority;
