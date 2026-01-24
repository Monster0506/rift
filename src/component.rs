use crate::job_manager::JobMessage;
use crate::key::Key;
use crate::layer::Layer;

/// Result of processing a key event
pub enum EventResult {
    /// Event was invalid or not handled
    Ignored,
    /// Event was handled
    Consumed,
    /// Event triggered an action with a payload
    Message(crate::message::AppMessage),
}

impl std::fmt::Debug for EventResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignored => write!(f, "Ignored"),
            Self::Consumed => write!(f, "Consumed"),
            Self::Message(_) => write!(f, "Message(...)"),
        }
    }
}

impl PartialEq for EventResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ignored, Self::Ignored) => true,
            (Self::Consumed, Self::Consumed) => true,
            // Cannot compare Message easily without deriving PartialEq on AppMessage
            // For now we act like they are different
            (Self::Message(_), Self::Message(_)) => false,
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

    /// Handle a message from a background job
    fn handle_job_message(&mut self, _msg: JobMessage) -> EventResult {
        EventResult::Ignored
    }

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
