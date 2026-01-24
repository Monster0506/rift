use crate::job_manager::Job;
use crate::notification::NotificationType;
use std::path::PathBuf;

/// Top-level application message
pub enum AppMessage {
    FileExplorer(FileExplorerMessage),
    CommandLine(CommandLineMessage),
    UndoTree(UndoTreeMessage),
    /// Generic actions that might be emitted by multiple components or don't fit a specific category
    Generic(GenericMessage),
}

/// Messages from the File Explorer component
pub enum FileExplorerMessage {
    SpawnJob(Box<dyn Job>),
    OpenFile(PathBuf),
    Notify(NotificationType, String),
    Close,
}

/// Messages from the Command Line component
pub enum CommandLineMessage {
    ExecuteCommand(String),
    ExecuteSearch(String),
    CancelMode, // Used for closing modals, clearing command line
}

/// Messages from the Undo Tree component
pub enum UndoTreeMessage {
    Goto(usize),    // Seq
    Preview(usize), // Seq
    Cancel,
}

/// Generic messages
pub enum GenericMessage {
    SpawnJob(Box<dyn Job>),
    Notify(NotificationType, String),
}
