use crate::key::Key;
use crate::layer::Layer;

/// Result of processing a key event
pub enum EventResult {
    /// Event was invalid or not handled
    Ignored,
    /// Event was handled
    Consumed,
    /// Event triggered an action with a payload
    Action(Box<dyn crate::editor::actions::EditorAction>),
}

impl std::fmt::Debug for EventResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignored => write!(f, "Ignored"),
            Self::Consumed => write!(f, "Consumed"),
            Self::Action(_) => write!(f, "Action(...)"),
        }
    }
}

impl PartialEq for EventResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ignored, Self::Ignored) => true,
            (Self::Consumed, Self::Consumed) => true,
            (Self::Action(_), Self::Action(_)) => false, // Cannot compare Action
            _ => false,
        }
    }
}

/// Common interface for all UI components (widgets)
pub trait Component {
    /// Handle input and return a result
    /// Returns EventResult::Consumed if the input was handled
    fn handle_input(&mut self, key: Key) -> EventResult;

    /// Render the component to the given layer
    fn render(&mut self, layer: &mut Layer);

    /// Get the cursor position for this component (absolute terminal coordinates)
    /// Returns None if the component doesn't want the cursor.
    fn cursor_position(&self) -> Option<(u16, u16)> {
        None
    }

    /// Downcast to concrete type
    fn as_any(&self) -> &dyn std::any::Any;

    /// Downcast to concrete type (mutable)
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}
