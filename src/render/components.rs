use crate::layer::LayerPriority;
use crate::render::{
    CommandDrawState, CompletionMenuDrawState, ContentDrawState, NotificationDrawState,
    StatusDrawState,
};

/// Position and size of a visual element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rect {
    row: usize,
    col: usize,
    height: usize,
    width: usize,
}

impl Rect {
    #[must_use]
    pub(super) const fn new(row: usize, col: usize, height: usize, width: usize) -> Self {
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
pub(super) enum Renderable {
    TextBuffer(ContentDrawState),
    StatusBar(StatusDrawState),
    Window(CommandDrawState),
    Notification(NotificationDrawState),
    CompletionMenu(CompletionMenuDrawState),
}

/// A component that defines the Z-ordering
pub(super) type Layer = LayerPriority;
