//! Editor actions, including movements and operations
use crate::command::Command;
use crate::search::{find_next, SearchDirection};

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

impl Motion {
    pub fn apply(
        self,
        buf: &mut crate::buffer::TextBuffer,
        display_map: Option<&crate::wrap::DisplayMap>,
        op_ctx: crate::wrap::OperatorContext,
        _tab_width: usize,
        viewport_height: usize,
        last_search_query: Option<&str>,
    ) {
        match self {
            Motion::Left => {
                buf.move_left();
            }
            Motion::Right => {
                buf.move_right();
            }
            Motion::Up => {
                if op_ctx == crate::wrap::OperatorContext::Move {
                    if let Some(dm) = display_map {
                        let new_pos = dm.visual_up(buf.cursor(), buf);
                        let _ = buf.set_cursor(new_pos);
                        return;
                    }
                }
                buf.move_up();
            }
            Motion::Down => {
                if op_ctx == crate::wrap::OperatorContext::Move {
                    if let Some(dm) = display_map {
                        let new_pos = dm.visual_down(buf.cursor(), buf);
                        let _ = buf.set_cursor(new_pos);
                        return;
                    }
                }
                buf.move_down();
            }
            Motion::StartOfLine => {
                buf.move_to_line_start();
            }
            Motion::EndOfLine => {
                buf.move_to_line_end();
            }
            Motion::StartOfFile => buf.move_to_start(),
            Motion::EndOfFile => buf.move_to_end(),
            Motion::PageUp => {
                for _ in 0..viewport_height {
                    buf.move_up();
                }
            }
            Motion::PageDown => {
                for _ in 0..viewport_height {
                    buf.move_down();
                }
            }
            Motion::NextWord => {
                buf.move_word_right();
            }
            Motion::PreviousWord => {
                buf.move_word_left();
            }
            Motion::NextBigWord => {
                buf.move_word_right();
            }
            Motion::PreviousBigWord => {
                buf.move_word_left();
            }
            Motion::NextParagraph => {
                buf.move_paragraph_forward();
            }
            Motion::PreviousParagraph => {
                buf.move_paragraph_backward();
            }
            Motion::NextSentence => {
                buf.move_sentence_forward();
            }
            Motion::PreviousSentence => {
                buf.move_sentence_backward();
            }
            Motion::NextMatch => {
                if let Some(query) = last_search_query {
                    let start = buf.cursor().saturating_add(1);
                    if let Ok((Some(m), _stats)) =
                        find_next(buf, start, query, SearchDirection::Forward)
                    {
                        let _ = buf.set_cursor(m.range.start);
                    }
                }
            }
            Motion::PreviousMatch => {
                if let Some(query) = last_search_query {
                    if let Ok((Some(m), _stats)) =
                        find_next(buf, buf.cursor(), query, SearchDirection::Backward)
                    {
                        let _ = buf.set_cursor(m.range.start);
                    }
                }
            }
        }
    }
}

use crate::error::RiftError;
use std::str::FromStr;

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
    EnterInsertModeAtLineStart,
    EnterInsertModeAtLineEnd,
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
    OpenMessages,
    ShowBufferList,
    ClearHighlights,
    ClearNotifications,
    ClearLastNotification,
    Checkpoint,
    /// Execute a command string (e.g. ":w", ":s/foo/bar")
    RunCommand(String),
    /// Jump to a 1-indexed line. 0 means last line.
    GotoLine(usize),
    /// Search forward for a pattern and jump to the first match.
    Search(String),
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
    /// Repeat last buffer mutation (dot-repeat)
    DotRepeat,
    QuitForce,
    OpenFile { path: Option<String>, force: bool },
    OpenDirectory(std::path::PathBuf),
    OpenTerminal(Option<String>),
    SplitWindow { direction: crate::split::tree::SplitDirection, subcommand: crate::command_line::commands::SplitSubcommand },
    UndoCount(Option<u64>),
    RedoCount(Option<u64>),
    UndoGoto(u64),
    NotificationClearAll,
}

/// Represents an action in the editor
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Editor actions
    Editor(EditorAction),
    /// Generic buffer-kind action, namespaced by kind
    Buffer(String),
    /// No action
    Noop,
}

impl FromStr for Action {
    type Err = RiftError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            // Movement
            "editor:move:left" => Ok(Action::Editor(EditorAction::Move(Motion::Left))),
            "editor:move:right" => Ok(Action::Editor(EditorAction::Move(Motion::Right))),
            "editor:move:up" => Ok(Action::Editor(EditorAction::Move(Motion::Up))),
            "editor:move:down" => Ok(Action::Editor(EditorAction::Move(Motion::Down))),
            "editor:move:line:start" => Ok(Action::Editor(EditorAction::Move(Motion::StartOfLine))),
            "editor:move:line:end" => Ok(Action::Editor(EditorAction::Move(Motion::EndOfLine))),
            "editor:move:file:start" => Ok(Action::Editor(EditorAction::Move(Motion::StartOfFile))),
            "editor:move:file:end" => Ok(Action::Editor(EditorAction::Move(Motion::EndOfFile))),
            "editor:move:page:up" => Ok(Action::Editor(EditorAction::Move(Motion::PageUp))),
            "editor:move:page:down" => Ok(Action::Editor(EditorAction::Move(Motion::PageDown))),
            "editor:move:word:next" => Ok(Action::Editor(EditorAction::Move(Motion::NextWord))),
            "editor:move:word:prev" => Ok(Action::Editor(EditorAction::Move(Motion::PreviousWord))),
            "editor:move:bigword:next" => Ok(Action::Editor(EditorAction::Move(Motion::NextBigWord))),
            "editor:move:bigword:prev" => Ok(Action::Editor(EditorAction::Move(Motion::PreviousBigWord))),
            "editor:move:paragraph:next" => Ok(Action::Editor(EditorAction::Move(Motion::NextParagraph))),
            "editor:move:paragraph:prev" => Ok(Action::Editor(EditorAction::Move(Motion::PreviousParagraph))),
            "editor:move:sentence:next" => Ok(Action::Editor(EditorAction::Move(Motion::NextSentence))),
            "editor:move:sentence:prev" => Ok(Action::Editor(EditorAction::Move(Motion::PreviousSentence))),
            "editor:move:match:next" => Ok(Action::Editor(EditorAction::Move(Motion::NextMatch))),
            "editor:move:match:prev" => Ok(Action::Editor(EditorAction::Move(Motion::PreviousMatch))),

            // Mode transitions
            "mode:normal" => Ok(Action::Editor(EditorAction::EnterNormalMode)),
            "mode:insert" => Ok(Action::Editor(EditorAction::EnterInsertMode)),
            "mode:insert_after" => Ok(Action::Editor(EditorAction::EnterInsertModeAfter)),
            "mode:insert_line_start" => {
                Ok(Action::Editor(EditorAction::EnterInsertModeAtLineStart))
            }
            "mode:insert_line_end" => Ok(Action::Editor(EditorAction::EnterInsertModeAtLineEnd)),
            "mode:command" => Ok(Action::Editor(EditorAction::EnterCommandMode)),
            "mode:search" => Ok(Action::Editor(EditorAction::EnterSearchMode)),

            // Buffer management
            "buffer:next" => Ok(Action::Editor(EditorAction::BufferNext)),
            "buffer:prev" => Ok(Action::Editor(EditorAction::BufferPrevious)),
            "buffer:list" => Ok(Action::Editor(EditorAction::ShowBufferList)),

            // Search
            "search:clear" => Ok(Action::Editor(EditorAction::ClearHighlights)),

            // Notifications
            "notifications:clear" => Ok(Action::Editor(EditorAction::ClearNotifications)),
            "notifications:clear_last" => Ok(Action::Editor(EditorAction::ClearLastNotification)),

            // History
            "history:undo" => Ok(Action::Editor(EditorAction::Undo)),
            "history:redo" => Ok(Action::Editor(EditorAction::Redo)),
            "history:checkpoint" => Ok(Action::Editor(EditorAction::Checkpoint)),

            // Feature openers
            "explorer:open" => Ok(Action::Editor(EditorAction::OpenExplorer)),
            "undotree:open" => Ok(Action::Editor(EditorAction::OpenUndoTree)),
            "terminal:open" => Ok(Action::Editor(EditorAction::OpenTerminal(None))),
            "messages:open" => Ok(Action::Editor(EditorAction::OpenMessages)),

            // Editor-only actions
            "editor:delete_line" => Ok(Action::Editor(EditorAction::DeleteLine)),
            "editor:delete_char" => Ok(Action::Editor(EditorAction::Delete(Motion::Right))),
            "editor:delete_back" => Ok(Action::Editor(EditorAction::Delete(Motion::Left))),
            "editor:toggle_debug" => Ok(Action::Editor(EditorAction::ToggleDebug)),
            "editor:redraw" => Ok(Action::Editor(EditorAction::Redraw)),
            "editor:save" => Ok(Action::Editor(EditorAction::Save)),
            "editor:save_and_quit" => Ok(Action::Editor(EditorAction::SaveAndQuit)),
            "editor:quit" => Ok(Action::Editor(EditorAction::Quit)),
            "editor:submit" => Ok(Action::Editor(EditorAction::Submit)),
            "editor:history_up" => Ok(Action::Editor(EditorAction::HistoryUp)),
            "editor:history_down" => Ok(Action::Editor(EditorAction::HistoryDown)),
            "editor:dot_repeat" => Ok(Action::Editor(EditorAction::DotRepeat)),

            // Navigation / search (parameterised — must precede Buffer catch-all)
            s if s.starts_with("editor:run:") => {
                Ok(Action::Editor(EditorAction::RunCommand(s["editor:run:".len()..].to_string())))
            }
            "editor:goto:last" => Ok(Action::Editor(EditorAction::GotoLine(0))),
            s if s.starts_with("editor:goto:line:") => {
                let n = s["editor:goto:line:".len()..].parse::<usize>().unwrap_or(0);
                Ok(Action::Editor(EditorAction::GotoLine(n)))
            }
            s if s.starts_with("editor:search:") => {
                Ok(Action::Editor(EditorAction::Search(s["editor:search:".len()..].to_string())))
            }

            // Multi-level editor sub-namespace: editor:X:Y forwards to Buffer action X:Y
            s if s.starts_with("editor:") && s.matches(':').count() >= 2 => {
                Ok(Action::Buffer(s["editor:".len()..].to_string()))
            }

            s if s.contains(':') && !s.starts_with("editor:") => {
                Ok(Action::Buffer(s.to_string()))
            }
            _ => Ok(Action::Noop),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
