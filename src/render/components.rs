use crate::layer::LayerPriority;
use crate::render::{
    CommandDrawState, CompletionMenuDrawState, ContentDrawState, NotificationDrawState,
    StatusDrawState,
};

/// Position and size of a visual element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) height: usize,
    pub(crate) width: usize,
}

impl Rect {
    #[must_use]
    pub(crate) const fn new(row: usize, col: usize, height: usize, width: usize) -> Self {
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
pub(crate) enum Renderable {
    TextBuffer(ContentDrawState),
    StatusBar(StatusDrawState),
    Window(CommandDrawState),
    Notification(NotificationDrawState),
    CompletionMenu(CompletionMenuDrawState),
}

/// A component that defines the Z-ordering
pub(crate) type Layer = LayerPriority;
