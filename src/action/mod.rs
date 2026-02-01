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
    /// Move to the next big word (whitespace delimited)
    NextBigWord,
    /// Move to the previous big word (whitespace delimited)
    PreviousBigWord,
    /// Move to the next paragraph
    NextParagraph,
    /// Move to the previous paragraph
    PreviousParagraph,
    /// Move to the next sentence
    NextSentence,
    /// Move to the previous sentence
    PreviousSentence,
    /// Move to the next search match
    NextMatch,
    /// Move to the previous search match
    PreviousMatch,
}

use crate::error::RiftError;
use std::str::FromStr;

/// File Explorer specific actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileExplorerAction {
    Close,
    Down,
    Up,
    Select,
    Parent,
    ToggleSelection,
    SelectAll,
    ClearSelection,
    Refresh,
    ToggleHidden,
    ToggleMetadata,
    NewFile,
    NewDir,
    Delete,
    Rename,
    Copy,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorType {
    Delete,
    Change,
    Yank,
    // Format, Comment, etc.
}

/// Editor specific actions (wraps commands or motions)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    Move(Motion),
    EnterInsertMode,
    EnterInsertModeAfter,
    EnterNormalMode,
    EnterCommandMode,
    EnterSearchMode,
    Delete(Motion),
    DeleteLine,
    InsertChar(char),
    BufferNext,
    BufferPrevious,
    ToggleDebug,
    Redraw,
    Save,
    SaveAndQuit,
    OpenExplorer,
    OpenUndoTree,
    ShowBufferList,
    ClearHighlights,
    ClearNotifications,
    ClearLastNotification,
    Checkpoint,
    /// Execute a command string (e.g. ":w", ":s/foo/bar")
    RunCommand(String),
    Undo,
    Redo,
    Quit,
    /// Pending Operator (d, c, y)
    Operator(OperatorType),
    /// Generic wrapper for other commands
    Command(Box<Command>),
    Submit,
    /// Navigate to previous (older) history entry
    HistoryUp,
    /// Navigate to next (newer) history entry
    HistoryDown,
}

/// Undotree specific actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoTreeAction {
    Close,
    Down,
    Up,
    Select,
    Top,
    Bottom,
}

/// Represents an action in the editor
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Editor actions
    Editor(EditorAction),
    /// File Explorer actions
    Explorer(FileExplorerAction),
    /// Undotree actions
    UndoTree(UndoTreeAction),
    /// No action
    Noop,
}

impl FromStr for Action {
    type Err = RiftError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            // Explorer Actions
            "explorer:close" => Ok(Action::Explorer(FileExplorerAction::Close)),
            "explorer:down" => Ok(Action::Explorer(FileExplorerAction::Down)),
            "explorer:up" => Ok(Action::Explorer(FileExplorerAction::Up)),
            "explorer:select" => Ok(Action::Explorer(FileExplorerAction::Select)),
            "explorer:parent" => Ok(Action::Explorer(FileExplorerAction::Parent)),
            "explorer:toggle_selection" => {
                Ok(Action::Explorer(FileExplorerAction::ToggleSelection))
            }
            "explorer:select_all" => Ok(Action::Explorer(FileExplorerAction::SelectAll)),
            "explorer:clear_selection" => Ok(Action::Explorer(FileExplorerAction::ClearSelection)),
            "explorer:refresh" => Ok(Action::Explorer(FileExplorerAction::Refresh)),
            "explorer:toggle_hidden" => Ok(Action::Explorer(FileExplorerAction::ToggleHidden)),
            "explorer:toggle_metadata" => Ok(Action::Explorer(FileExplorerAction::ToggleMetadata)),
            "explorer:new_file" => Ok(Action::Explorer(FileExplorerAction::NewFile)),
            "explorer:new_dir" => Ok(Action::Explorer(FileExplorerAction::NewDir)),
            "explorer:delete" => Ok(Action::Explorer(FileExplorerAction::Delete)),
            "explorer:rename" => Ok(Action::Explorer(FileExplorerAction::Rename)),
            "explorer:copy" => Ok(Action::Explorer(FileExplorerAction::Copy)),
            "explorer:top" => Ok(Action::Explorer(FileExplorerAction::Top)),
            "explorer:bottom" => Ok(Action::Explorer(FileExplorerAction::Bottom)),

            // Undotree Actions
            "undotree:close" => Ok(Action::UndoTree(UndoTreeAction::Close)),
            "undotree:down" => Ok(Action::UndoTree(UndoTreeAction::Down)),
            "undotree:up" => Ok(Action::UndoTree(UndoTreeAction::Up)),
            "undotree:select" => Ok(Action::UndoTree(UndoTreeAction::Select)),
            "undotree:top" => Ok(Action::UndoTree(UndoTreeAction::Top)),
            "undotree:bottom" => Ok(Action::UndoTree(UndoTreeAction::Bottom)),

            // Editor Actions - Movement
            "editor:move_left" => Ok(Action::Editor(EditorAction::Move(Motion::Left))),
            "editor:move_right" => Ok(Action::Editor(EditorAction::Move(Motion::Right))),
            "editor:move_up" => Ok(Action::Editor(EditorAction::Move(Motion::Up))),
            "editor:move_down" => Ok(Action::Editor(EditorAction::Move(Motion::Down))),
            "editor:move_start_of_line" => {
                Ok(Action::Editor(EditorAction::Move(Motion::StartOfLine)))
            }
            "editor:move_end_of_line" => Ok(Action::Editor(EditorAction::Move(Motion::EndOfLine))),

            // Editor Actions - General
            "editor:enter_insert_mode" => Ok(Action::Editor(EditorAction::EnterInsertMode)),
            "editor:enter_insert_mode_after" => {
                Ok(Action::Editor(EditorAction::EnterInsertModeAfter))
            }
            "editor:enter_normal_mode" => Ok(Action::Editor(EditorAction::EnterNormalMode)),
            "editor:enter_command_mode" => Ok(Action::Editor(EditorAction::EnterCommandMode)),
            "editor:enter_search_mode" => Ok(Action::Editor(EditorAction::EnterSearchMode)),
            "editor:undo" => Ok(Action::Editor(EditorAction::Undo)),
            "editor:redo" => Ok(Action::Editor(EditorAction::Redo)),
            "editor:quit" => Ok(Action::Editor(EditorAction::Quit)),
            "editor:buffer_next" => Ok(Action::Editor(EditorAction::BufferNext)),
            "editor:buffer_previous" => Ok(Action::Editor(EditorAction::BufferPrevious)),
            "editor:delete_line" => Ok(Action::Editor(EditorAction::DeleteLine)),
            "editor:delete_char" => Ok(Action::Editor(EditorAction::Delete(Motion::Right))),
            "editor:delete_back" => Ok(Action::Editor(EditorAction::Delete(Motion::Left))),
            "editor:toggle_debug" => Ok(Action::Editor(EditorAction::ToggleDebug)),
            "editor:redraw" => Ok(Action::Editor(EditorAction::Redraw)),
            "editor:save" => Ok(Action::Editor(EditorAction::Save)),
            "editor:save_and_quit" => Ok(Action::Editor(EditorAction::SaveAndQuit)),
            "editor:open_explorer" => Ok(Action::Editor(EditorAction::OpenExplorer)),
            "editor:open_undotree" => Ok(Action::Editor(EditorAction::OpenUndoTree)),
            "editor:show_buffer_list" => Ok(Action::Editor(EditorAction::ShowBufferList)),
            "editor:clear_highlights" => Ok(Action::Editor(EditorAction::ClearHighlights)),
            "editor:clear_notifications" => Ok(Action::Editor(EditorAction::ClearNotifications)),
            "editor:clear_last_notification" => {
                Ok(Action::Editor(EditorAction::ClearLastNotification))
            }
            "editor:checkpoint" => Ok(Action::Editor(EditorAction::Checkpoint)),
            "editor:submit" => Ok(Action::Editor(EditorAction::Submit)),

            _ => Ok(Action::Noop),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
