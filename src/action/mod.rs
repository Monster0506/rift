//! Editor actions, including movements and operations
use crate::command::Command;

/// Represents a count for a command or motion
pub type Count = usize;

/// Represents a motion in the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Move left by one character
    Left,
    /// Move right by one character
    Right,
    /// Move up by one line
    Up,
    /// Move down by one line
    Down,
    /// Move to the start of the line
    StartOfLine,
    /// Move to the end of the line
    EndOfLine,
    /// Move to the start of the file
    StartOfFile,
    /// Move to the end of the file
    EndOfFile,
    /// Move up by one page
    PageUp,
    /// Move down by one page
    PageDown,
    /// Move to the next word
    NextWord,
    /// Move to the previous word
    PreviousWord,
    /// Move to the next paragraph
    NextParagraph,
    /// Move to the previous paragraph
    PreviousParagraph,
}

/// Represents an action in the editor
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Perform a motion
    Move(Motion),
    /// Enter insert mode
    EnterInsertMode,
    /// Enter command mode
    EnterCommandMode,
    /// Execute a command
    Execute(Box<Command>),
    /// No action
    Noop,
}
