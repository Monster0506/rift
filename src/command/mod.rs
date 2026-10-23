//! Editor commands: immutable, platform-agnostic editor-level intent (not
//! key-level input), self-contained enough to apply without extra context.
use crate::action::Motion;

#[cfg(test)]
mod input;

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
    OpenLineBelow,
    OpenLineAbove,
    Delete(Motion, usize),
    Change(Motion, usize),
    DeleteForward,
    DeleteBackward,
    DeleteLine(usize),
    ChangeLine(usize),
    InsertChar(char),
    /// Replace `count` chars at cursor with `ch` (`r<ch>` with optional count).
    ReplaceChar(char, usize),
    /// Replace-mode typing: overwrite the char under the cursor, or insert at line end.
    OverwriteChar(char),
    /// Replace-mode Backspace: put back the displaced char, or delete an appended one.
    ReplaceBackspace(Option<crate::character::Character>),
    /// Enter Replace mode (R: each char overwrites instead of inserting).
    EnterReplaceMode,
    /// Delete an existing surround pair, keeping its contents (`sd<ch>`).
    /// The trailing count repeats the delimiter char on each side.
    DeleteSurround(char, usize),
    /// Replace an existing surround pair's delimiters (`sc<from><to>`).
    /// The trailing count repeats the delimiter char on each side.
    ChangeSurround(char, char, usize),
    /// Wrap a motion/text-object range in delimiters (`sg<motion><ch>`).
    /// The motion count resolves the range; the trailing count repeats the delimiter.
    AddSurround(Motion, usize, char, usize),

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
    pub(crate) const fn is_mutating(&self) -> bool {
        matches!(
            self,
            Self::DeleteForward
                | Self::DeleteBackward
                | Self::DeleteLine(_)
                | Self::Delete(_, _)
                | Self::Change(_, _)
                | Self::ChangeLine(_)
                | Self::OpenLineBelow
                | Self::OpenLineAbove
                | Self::InsertChar(_)
                | Self::ReplaceChar(_, _)
                | Self::OverwriteChar(_)
                | Self::ReplaceBackspace(_)
                | Self::DeleteSurround(_, _)
                | Self::ChangeSurround(_, _, _)
                | Self::AddSurround(_, _, _, _)
                | Self::Undo
                | Self::Redo
        )
    }

    /// Check if command should be recorded for dot-repeat
    #[must_use]
    pub(crate) const fn is_repeatable(&self) -> bool {
        self.is_mutating() && !matches!(self, Self::Undo | Self::Redo)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
