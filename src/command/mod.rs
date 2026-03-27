//! Editor commands
//!
//! ## command/ Invariants
//!
//! - `Command` represents editor-level intent, not key-level input.
//! - Commands contain no terminal- or platform-specific concepts.
//! - All data required to apply a command is contained within the command.
//! - Commands are immutable once created.
//! - Adding a new command requires explicit executor support.
use crate::action::Motion;

pub mod input;

/// Editor commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    // Movement
    Move(Motion, usize),

    // Editing
    EnterInsertMode,
    EnterInsertModeAfter,
    EnterInsertModeAtLineStart,
    EnterInsertModeAtLineEnd,
    Delete(Motion, usize),
    Change(Motion, usize),
    DeleteForward,
    DeleteBackward,
    DeleteLine,
    ChangeLine,
    InsertChar(char),

    // Mode transitions
    EnterCommandMode,
    EnterSearchMode,

    // Search
    ExecuteSearch,
    NextMatch,
    PreviousMatch,

    // Command line editing
    AppendToCommandLine(char),
    DeleteFromCommandLine,
    ExecuteCommandLine,

    // Buffer/Tab management
    BufferNext,
    BufferPrevious,

    // History
    Undo,
    Redo,

    // Repeat
    DotRepeat,

    // Completion
    TabComplete,
    TabCompletePrev,

    // Control
    Quit,
    Noop,
}

impl Command {
    /// Check if command mutates the buffer content
    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            Command::DeleteForward
                | Command::DeleteBackward
                | Command::DeleteLine
                | Command::Delete(_, _)
                | Command::Change(_, _)
                | Command::ChangeLine
                | Command::InsertChar(_)
                | Command::Undo
                | Command::Redo
        )
    }

    /// Check if command should be recorded for dot-repeat
    #[must_use]
    pub fn is_repeatable(&self) -> bool {
        self.is_mutating() && !matches!(self, Command::Undo | Command::Redo)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
