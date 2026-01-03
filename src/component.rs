use crate::key::Key;
use crate::layer::Layer;

/// Result of processing a key event
#[derive(Debug)]
pub enum EventResult {
    /// Event was invalid or not handled
    Ignored,
    /// Event was handled
    Consumed,
    /// Event triggered an action with a payload
    Action(Box<dyn std::any::Any>),
}

impl PartialEq for EventResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ignored, Self::Ignored) => true,
            (Self::Consumed, Self::Consumed) => true,
            (Self::Action(_), Self::Action(_)) => false, // Cannot compare Any
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
}
