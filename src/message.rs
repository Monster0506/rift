use crate::job_manager::Job;
use crate::notification::NotificationType;

/// Top-level application message
pub enum AppMessage {
    CommandLine(CommandLineMessage),
    /// Generic actions that might be emitted by multiple components or don't fit a specific category
    Generic(GenericMessage),
}

/// Messages from the Command Line component
pub enum CommandLineMessage {
    ExecuteCommand(String),
    ExecuteSearch(String),
    CancelMode,                // Used for closing modals, clearing command line
    RequestCompletion(String), // Tab pressed; String is current command line content
}

/// Generic messages
pub enum GenericMessage {
    SpawnJob(Box<dyn Job>),
    Notify(NotificationType, String),
}
