//! Editor commands: immutable, platform-agnostic editor-level intent (not
//! key-level input), self-contained enough to apply without extra context.
use crate::action::Motion;

#[cfg(test)]
mod input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InsertEntry {
    AtCursor,
    AfterCursor,
    LineStart,
    LineEnd,
    NewLineBelow,
    NewLineAbove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    // Movement
    MoveBy(Motion, usize),

    // Editing
    BeginInsert(InsertEntry),
    DeleteMotion(Motion, usize),
    Change(Motion, usize),
    DeleteRows(usize),
    ChangeLine(usize),
    TypeChar(char),
    /// Replace `count` chars at cursor with `ch` (`r<ch>` with optional count).
    ReplaceChar(char, usize),
    /// Replace-mode typing: overwrite the char under the cursor, or insert at line end.
    OverwriteChar(char),
    /// Replace-mode Backspace: put back the displaced char, or delete an appended one.
    ReplaceBackspace(Option<crate::character::Character>),
    /// Enter Replace mode (R: each char overwrites instead of inserting).
    BeginReplace,
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
    ReadExLine,
    ReadSearchPattern,

    // Search
    ExecuteSearch,

    // Command line editing
    AppendToCommandLine(char),
    DeleteFromCommandLine,
    ExecuteCommandLine,

    // History
    ReplayUndo,
    ReplayRedo,

    // Completion
    TabComplete,
    TabCompletePrev,

    // Control
    Noop,
}

impl Command {
    /// Check if command mutates the buffer content
    #[must_use]
    pub(crate) const fn is_mutating(&self) -> bool {
        matches!(
            self,
            Self::DeleteRows(_)
                | Self::DeleteMotion(_, _)
                | Self::Change(_, _)
                | Self::ChangeLine(_)
                | Self::BeginInsert(InsertEntry::NewLineBelow | InsertEntry::NewLineAbove)
                | Self::TypeChar(_)
                | Self::ReplaceChar(_, _)
                | Self::OverwriteChar(_)
                | Self::ReplaceBackspace(_)
                | Self::DeleteSurround(_, _)
                | Self::ChangeSurround(_, _, _)
                | Self::AddSurround(_, _, _, _)
                | Self::ReplayUndo
                | Self::ReplayRedo
        )
    }

    /// Check if command should be recorded for dot-repeat
    #[must_use]
    pub(crate) const fn is_repeatable(&self) -> bool {
        self.is_mutating() && !matches!(self, Self::ReplayUndo | Self::ReplayRedo)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
