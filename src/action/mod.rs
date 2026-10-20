//! Editor actions, including movements and operations
use crate::command::Command;
use crate::search::{find_next, SearchDirection};
use crate::text_objects::TextObjectSpec;

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
    /// Move to the end of the current or next word (vim's `e`)
    WordEnd,
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
    /// Find next occurrence of a character on the current line (f)
    FindCharForward(char),
    /// Find previous occurrence of a character on the current line (F)
    FindCharBackward(char),
    /// Move to one position before the next occurrence of a character on the line (t)
    TillCharForward(char),
    /// Move to one position after the previous occurrence of a character on the line (T)
    TillCharBackward(char),
    /// Repeat last f/F/t/T in the same direction (n / vim's ;)
    RepeatFindForward,
    /// Repeat last f/F/t/T in the opposite direction (N / vim's ,)
    RepeatFindBackward,
    /// Move to the start of a specific 0-based line (linewise, used by G/nG operators)
    ToLine(usize),
    /// Text object (used in operator-pending mode only)
    TextObject(TextObjectSpec),
}

impl Motion {
    pub fn apply(
        self,
        buf: &mut crate::buffer::TextBuffer,
        display_map: Option<&mut crate::wrap::DisplayMap>,
        op_ctx: crate::wrap::OperatorContext,
        viewport_height: usize,
        last_search_query: Option<&str>,
    ) {
        match self {
            Self::Left => {
                buf.move_left();
            }
            Self::Right => {
                buf.move_right();
            }
            Self::Up => {
                if op_ctx == crate::wrap::OperatorContext::Move {
                    if let Some(dm) = display_map {
                        let col =
                            buf.latch_desired_col(dm.char_to_visual_col_ext(buf.cursor(), buf));
                        let new_pos = dm.visual_up_to_col_ext(buf.cursor(), col, buf);
                        let _ = buf.set_cursor(new_pos);
                        return;
                    }
                }
                buf.move_up();
            }
            Self::Down => {
                if op_ctx == crate::wrap::OperatorContext::Move {
                    if let Some(dm) = display_map {
                        let col =
                            buf.latch_desired_col(dm.char_to_visual_col_ext(buf.cursor(), buf));
                        let new_pos = dm.visual_down_to_col_ext(buf.cursor(), col, buf);
                        let _ = buf.set_cursor(new_pos);
                        return;
                    }
                }
                buf.move_down();
            }
            Self::StartOfLine => {
                buf.move_to_line_start();
            }
            Self::EndOfLine => {
                buf.move_to_line_end();
            }
            Self::StartOfFile => buf.move_to_start(),
            Self::EndOfFile => buf.move_to_end(),
            Self::PageUp => {
                for _ in 0..viewport_height {
                    buf.move_up();
                }
            }
            Self::PageDown => {
                for _ in 0..viewport_height {
                    buf.move_down();
                }
            }
            Self::NextWord => {
                buf.move_word_right();
            }
            Self::PreviousWord => {
                buf.move_word_left();
            }
            Self::WordEnd => {
                buf.move_word_end();
            }
            Self::NextBigWord => {
                buf.move_big_word_right();
            }
            Self::PreviousBigWord => {
                buf.move_big_word_left();
            }
            Self::NextParagraph => {
                buf.move_paragraph_forward();
            }
            Self::PreviousParagraph => {
                buf.move_paragraph_backward();
            }
            Self::NextSentence => {
                buf.move_sentence_forward();
            }
            Self::PreviousSentence => {
                buf.move_sentence_backward();
            }
            Self::NextMatch => jump_to_match(buf, last_search_query, SearchDirection::Forward),
            Self::PreviousMatch => {
                jump_to_match(buf, last_search_query, SearchDirection::Backward);
            }
            Self::TillCharForward(ch) => till_char_forward(buf, ch, op_ctx),
            Self::TillCharBackward(ch) => till_char_backward(buf, ch, op_ctx),
            // Text objects resolve at operator level; normal mode does not move.
            Self::RepeatFindForward | Self::RepeatFindBackward | Self::TextObject(_) => {}
            Self::ToLine(line_idx) => {
                buf.clear_desired_col();
                let start = buf.line_index.get_start(line_idx).unwrap_or(0);
                let _ = buf.set_cursor(start);
            }
            Self::FindCharForward(ch) => find_char_forward(buf, ch),
            Self::FindCharBackward(ch) => find_char_backward(buf, ch),
        }
    }
}

fn till_char_forward(
    buf: &mut crate::buffer::TextBuffer,
    ch: char,
    op_ctx: crate::wrap::OperatorContext,
) {
    buf.clear_desired_col();
    let target = crate::character::Character::from(ch);
    let current_line = buf.get_line();
    let total_len = buf.len();
    let line_end = buf
        .line_index
        .get_end(current_line, total_len)
        .unwrap_or(total_len);
    let cursor = buf.cursor();
    let start = if buf.char_at(cursor + 1) == Some(target) {
        cursor + 2
    } else {
        cursor + 1
    };
    for pos in start..line_end {
        if buf.char_at(pos) == Some(target) {
            let land = if op_ctx == crate::wrap::OperatorContext::Operator {
                pos
            } else {
                pos.saturating_sub(1)
            };
            let _ = buf.set_cursor(land);
            return;
        }
    }
}

fn till_char_backward(
    buf: &mut crate::buffer::TextBuffer,
    ch: char,
    op_ctx: crate::wrap::OperatorContext,
) {
    buf.clear_desired_col();
    let target = crate::character::Character::from(ch);
    let current_line = buf.get_line();
    let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
    let cursor = buf.cursor();
    if cursor > line_start {
        let end = if cursor > 0 && buf.char_at(cursor - 1) == Some(target) {
            cursor - 1
        } else {
            cursor
        };
        if end > line_start {
            for pos in (line_start..end).rev() {
                if buf.char_at(pos) == Some(target) {
                    let land = if op_ctx == crate::wrap::OperatorContext::Operator {
                        pos
                    } else {
                        pos + 1
                    };
                    let _ = buf.set_cursor(land);
                    return;
                }
            }
        }
    }
}

fn find_char_forward(buf: &mut crate::buffer::TextBuffer, ch: char) {
    buf.clear_desired_col();
    let target = crate::character::Character::from(ch);
    let current_line = buf.get_line();
    let total_len = buf.len();
    let line_end = buf
        .line_index
        .get_end(current_line, total_len)
        .unwrap_or(total_len);
    let start = buf.cursor() + 1;
    for pos in start..line_end {
        if buf.char_at(pos) == Some(target) {
            let _ = buf.set_cursor(pos);
            return;
        }
    }
}

fn find_char_backward(buf: &mut crate::buffer::TextBuffer, ch: char) {
    buf.clear_desired_col();
    let target = crate::character::Character::from(ch);
    let current_line = buf.get_line();
    let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
    let cursor = buf.cursor();
    if cursor > line_start {
        for pos in (line_start..cursor).rev() {
            if buf.char_at(pos) == Some(target) {
                let _ = buf.set_cursor(pos);
                return;
            }
        }
    }
}

fn jump_to_match(
    buf: &mut crate::buffer::TextBuffer,
    last_search_query: Option<&str>,
    direction: SearchDirection,
) {
    buf.clear_desired_col();
    let Some(query) = last_search_query else {
        return;
    };
    let start = match direction {
        SearchDirection::Forward => buf.cursor().saturating_add(1),
        SearchDirection::Backward => buf.cursor(),
    };
    if let Ok((Some(m), _stats)) = find_next(buf, start, query, direction) {
        let _ = buf.set_cursor(m.range.start);
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
    OpenLineBelow,
    OpenLineAbove,
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
    Reload,
    Save,
    SaveAndQuit,
    OpenExplorer,
    OpenUndoTree,
    OpenMessages,
    OpenClipboard,
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
    QuitKey,
    OpenFile {
        path: Option<String>,
        force: bool,
    },
    OpenDirectory(std::path::PathBuf),
    OpenTerminal(Option<String>),
    SplitWindow {
        direction: crate::split::tree::SplitDirection,
        subcommand: crate::command_line::commands::SplitSubcommand,
    },
    UndoCount(Option<u64>),
    RedoCount(Option<u64>),
    UndoGoto(u64),
    NotificationClearAll,
    /// Invoke a registered plugin action by ID.
    PluginAction(String),
    /// Paste from the clipboard ring (p = after cursor, P = before)
    Put {
        before: bool,
    },
    /// Cycle through clipboard ring entries, replacing the last paste.
    /// `forward` = true advances to older entries (C-n), false goes back (C-p).
    CyclePaste {
        forward: bool,
    },
    /// Paste directly from the system clipboard
    PutSystemClipboard {
        before: bool,
    },
    /// Toggle display of hidden files (dot-files) in the file explorer
    ExplorerToggleHidden,
    /// Exit terminal insert mode (return to Normal). Default: Ctrl+\
    ExitTerminalMode,
    /// Scroll the terminal scrollback by `lines` (negative = up, positive = down).
    TerminalScrollback(i32),
    /// Enter char-pending state for f/F/t/T (next keypress becomes the target char)
    FindCharPending {
        forward: bool,
        till: bool,
    },
    /// Enter char-pending state for r (next keypress replaces count chars at cursor)
    ReplaceCharPending,
    /// Enter Replace mode (R: each char overwrites instead of inserting)
    EnterReplaceMode,
    /// LSP: go to definition under cursor (gd)
    LspGotoDefinition,
    /// LSP: go to references under cursor (gr)
    LspReferences,
    /// LSP: show hover documentation (K)
    LspHover,
    /// LSP: rename symbol under cursor
    LspRename,
    /// LSP: show code actions
    LspCodeAction,
    /// LSP: format current document
    LspFormat,
    /// LSP: jump to next diagnostic
    LspDiagnosticNext,
    /// LSP: jump to previous diagnostic
    LspDiagnosticPrev,
    /// LSP: open diagnostics panel (`<space>e`)
    LspDiagnosticsPanel,
    /// Activate the interactive annotation under the cursor (default action).
    ActivateAnnotation,
    /// Activate a specific verb on the annotation under the cursor, letting
    /// distinct keybinds drive distinct actions on one annotation.
    ActivateAnnotationVerb(String),
    /// Move to the next interactive annotation.
    NextInteractiveAnnotation,
    /// Move to the previous interactive annotation.
    PrevInteractiveAnnotation,
    /// Begin surround grammar (sd/sc/sg) as a top-level Normal-mode leader.
    SurroundStart,
    /// `g` pressed under `OperatorPending` while an `sg` add-surround is in
    /// progress: `sgg<ch>` doubling shorthand for "surround the current line".
    SurroundGiveLine,
    /// `v`: enter/resume charwise Visual selection.
    EnterVisualChar,
    /// `V`: enter/resume linewise Visual selection.
    EnterVisualLine,
    /// `Ctrl-V`: enter/resume blockwise Visual selection.
    EnterVisualBlock,
    /// `o`/`O` in Visual: swap which end (anchor vs cursor) is active.
    VisualSwapEnds,
    /// `m` (Normal): bank the next occurrence of the last-banked region's text.
    RegionBankOccurrenceNext,
    /// `M` (Normal): bank the previous occurrence of the last-banked region's text.
    RegionBankOccurrencePrev,
    /// Dot-repeat follow-up for `sg`: re-wrap rebuilt regions with `ch`.
    /// Synthetic action, not reachable from keymap.
    AddSurroundToSet {
        ch: char,
        delim_count: usize,
    },
    /// `<Space>` in Visual: grow the active region to the next enclosing
    /// text object.
    ExpandRegion,
    /// `<Shift-Space>` in Visual: undo the last `<Space>` expand step.
    ShrinkRegion,
    /// `gv`: toggle the regions list window.
    ToggleRegionsWindow,
    /// `j` inside the regions window: move down, live-jump the preview.
    RegionsListDown,
    /// `k` inside the regions window: move up, live-jump the preview.
    RegionsListUp,
    /// `Enter` inside the regions window: jump and return focus to the buffer.
    RegionsListSelect,
    /// `x` inside the regions window: drop that entry from the set.
    RegionsListDrop,
    /// `<Space>g`: open the git status buffer for the current repository.
    GitStatus,
    /// `cc` in the status buffer: open an empty commit message buffer.
    GitCommitNew,
    /// `ca`/`cw` in the status buffer: amend/reword HEAD's commit message.
    GitCommitAmend,
    /// `cf` in the status buffer: `git commit --fixup=HEAD` from staged changes.
    GitCommitFixup,
    /// `r` on a commit in the Log buffer: start a rebase from that commit
    /// onward (base = its parent).
    GitRebaseFromLogCommit,
    /// Abort an in-progress rebase and return to the original branch.
    GitRebaseAbort,
    /// `K`/`J` on a rebase-todo commit: move it up/down one slot.
    GitRebaseMoveUp,
    GitRebaseMoveDown,
    /// `p`/`s`/`f`/`e` on a rebase-todo commit: set its verb directly.
    GitRebaseSetPick,
    GitRebaseSetSquash,
    GitRebaseSetFixup,
    GitRebaseSetEdit,
    /// `dd` on a rebase-todo commit: drop it from the plan.
    GitRebaseDrop,
    /// `Enter`/`=` on a rebase-todo commit: toggle its inline body preview.
    GitRebaseToggleFold,
    /// `c`/`r` on a rebase-todo commit: open its message in the reword
    /// sub-editor (planning-time;  nothing's cherry-picked yet).
    GitRebaseOpenMessage,
    /// `g?` in any git buffer (status/log/blame/rebase todo): show its key
    /// reference as a scratch buffer.
    GitHelp,
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
            "editor:move:left" => Ok(Self::Editor(EditorAction::Move(Motion::Left))),
            "editor:move:right" => Ok(Self::Editor(EditorAction::Move(Motion::Right))),
            "editor:move:up" => Ok(Self::Editor(EditorAction::Move(Motion::Up))),
            "editor:move:down" => Ok(Self::Editor(EditorAction::Move(Motion::Down))),
            "editor:move:line:start" => Ok(Self::Editor(EditorAction::Move(Motion::StartOfLine))),
            "editor:move:line:end" => Ok(Self::Editor(EditorAction::Move(Motion::EndOfLine))),
            "editor:move:file:start" => Ok(Self::Editor(EditorAction::Move(Motion::StartOfFile))),
            "editor:move:file:end" => Ok(Self::Editor(EditorAction::Move(Motion::EndOfFile))),
            "editor:move:page:up" => Ok(Self::Editor(EditorAction::Move(Motion::PageUp))),
            "editor:move:page:down" => Ok(Self::Editor(EditorAction::Move(Motion::PageDown))),
            "editor:move:word:next" => Ok(Self::Editor(EditorAction::Move(Motion::NextWord))),
            "editor:move:word:prev" => Ok(Self::Editor(EditorAction::Move(Motion::PreviousWord))),
            "editor:move:bigword:next" => Ok(Self::Editor(EditorAction::Move(Motion::NextBigWord))),
            "editor:move:bigword:prev" => {
                Ok(Self::Editor(EditorAction::Move(Motion::PreviousBigWord)))
            }
            "editor:move:paragraph:next" => {
                Ok(Self::Editor(EditorAction::Move(Motion::NextParagraph)))
            }
            "editor:move:paragraph:prev" => {
                Ok(Self::Editor(EditorAction::Move(Motion::PreviousParagraph)))
            }
            "editor:move:sentence:next" => {
                Ok(Self::Editor(EditorAction::Move(Motion::NextSentence)))
            }
            "editor:move:sentence:prev" => {
                Ok(Self::Editor(EditorAction::Move(Motion::PreviousSentence)))
            }
            "editor:move:match:next" => Ok(Self::Editor(EditorAction::Move(Motion::NextMatch))),
            "editor:move:match:prev" => Ok(Self::Editor(EditorAction::Move(Motion::PreviousMatch))),

            // Mode transitions
            "mode:normal" => Ok(Self::Editor(EditorAction::EnterNormalMode)),
            "mode:insert" => Ok(Self::Editor(EditorAction::EnterInsertMode)),
            "mode:insert_after" => Ok(Self::Editor(EditorAction::EnterInsertModeAfter)),
            "mode:insert_line_start" => Ok(Self::Editor(EditorAction::EnterInsertModeAtLineStart)),
            "mode:insert_line_end" => Ok(Self::Editor(EditorAction::EnterInsertModeAtLineEnd)),
            "mode:open_line_below" => Ok(Self::Editor(EditorAction::OpenLineBelow)),
            "mode:open_line_above" => Ok(Self::Editor(EditorAction::OpenLineAbove)),
            "mode:command" => Ok(Self::Editor(EditorAction::EnterCommandMode)),
            "mode:search" => Ok(Self::Editor(EditorAction::EnterSearchMode)),
            _ => Ok(Self::parse_rest(s)),
        }
    }
}

impl Action {
    fn parse_rest(s: &str) -> Self {
        match s {
            // Buffer management
            "buffer:next" => Self::Editor(EditorAction::BufferNext),
            "buffer:prev" => Self::Editor(EditorAction::BufferPrevious),
            "buffer:list" => Self::Editor(EditorAction::ShowBufferList),

            // Search
            "search:clear" => Self::Editor(EditorAction::ClearHighlights),

            // Notifications
            "notifications:clear" => Self::Editor(EditorAction::ClearNotifications),
            "notifications:clear_last" => Self::Editor(EditorAction::ClearLastNotification),

            // History
            "history:undo" => Self::Editor(EditorAction::Undo),
            "history:redo" => Self::Editor(EditorAction::Redo),
            "history:checkpoint" => Self::Editor(EditorAction::Checkpoint),

            // Feature openers
            "explorer:open" => Self::Editor(EditorAction::OpenExplorer),
            "explorer:toggle_hidden" => Self::Editor(EditorAction::ExplorerToggleHidden),
            "terminal:exit" => Self::Editor(EditorAction::ExitTerminalMode),
            "terminal:scroll_up" => Self::Editor(EditorAction::TerminalScrollback(5)),
            "terminal:scroll_down" => Self::Editor(EditorAction::TerminalScrollback(-5)),
            "terminal:scroll_page_up" => Self::Editor(EditorAction::TerminalScrollback(20)),
            "terminal:scroll_page_down" => Self::Editor(EditorAction::TerminalScrollback(-20)),
            "undotree:open" => Self::Editor(EditorAction::OpenUndoTree),
            "terminal:open" => Self::Editor(EditorAction::OpenTerminal(None)),
            "messages:open" => Self::Editor(EditorAction::OpenMessages),
            "clipboard:open" => Self::Editor(EditorAction::OpenClipboard),

            // Editor-only actions
            "editor:delete_line" => Self::Editor(EditorAction::DeleteLine),
            "editor:delete_char" => Self::Editor(EditorAction::Delete(Motion::Right)),
            "editor:delete_back" => Self::Editor(EditorAction::Delete(Motion::Left)),
            "editor:toggle_debug" => Self::Editor(EditorAction::ToggleDebug),
            "editor:redraw" => Self::Editor(EditorAction::Redraw),
            "editor:reload" => Self::Editor(EditorAction::Reload),
            "editor:save" => Self::Editor(EditorAction::Save),
            "editor:save_and_quit" => Self::Editor(EditorAction::SaveAndQuit),
            "editor:quit" => Self::Editor(EditorAction::Quit),
            "editor:submit" => Self::Editor(EditorAction::Submit),
            "editor:history_up" => Self::Editor(EditorAction::HistoryUp),
            "editor:history_down" => Self::Editor(EditorAction::HistoryDown),
            "editor:dot_repeat" => Self::Editor(EditorAction::DotRepeat),

            // Navigation / search (parameterised;  must precede Buffer catch-all)
            s if s.starts_with("editor:run:") => Self::Editor(EditorAction::RunCommand(
                s.strip_prefix("editor:run:")
                    .unwrap_or_default()
                    .to_string(),
            )),
            "editor:goto:last" => Self::Editor(EditorAction::GotoLine(0)),
            s if s.starts_with("editor:goto:line:") => {
                let n = s
                    .strip_prefix("editor:goto:line:")
                    .unwrap_or_default()
                    .parse::<usize>()
                    .unwrap_or(0);
                Self::Editor(EditorAction::GotoLine(n))
            }
            s if s.starts_with("editor:search:") => Self::Editor(EditorAction::Search(
                s.strip_prefix("editor:search:")
                    .unwrap_or_default()
                    .to_string(),
            )),

            // Multi-level editor sub-namespace: editor:X:Y forwards to Buffer action X:Y
            s if s.starts_with("editor:") && s.matches(':').count() >= 2 => {
                Self::Buffer(s.strip_prefix("editor:").unwrap_or_default().to_string())
            }

            // LSP actions
            "lsp:definition" => Self::Editor(EditorAction::LspGotoDefinition),
            "lsp:references" => Self::Editor(EditorAction::LspReferences),
            "lsp:hover" => Self::Editor(EditorAction::LspHover),
            "lsp:rename" => Self::Editor(EditorAction::LspRename),
            "lsp:code_action" => Self::Editor(EditorAction::LspCodeAction),
            "lsp:format" => Self::Editor(EditorAction::LspFormat),
            "lsp:diagnostic:next" => Self::Editor(EditorAction::LspDiagnosticNext),
            "lsp:diagnostic:prev" => Self::Editor(EditorAction::LspDiagnosticPrev),
            "annotation:activate" => Self::Editor(EditorAction::ActivateAnnotation),
            s if s.starts_with("annotation:activate:") => {
                Self::Editor(EditorAction::ActivateAnnotationVerb(
                    s.strip_prefix("annotation:activate:")
                        .unwrap_or_default()
                        .to_string(),
                ))
            }
            "annotation:next" => Self::Editor(EditorAction::NextInteractiveAnnotation),
            "annotation:prev" => Self::Editor(EditorAction::PrevInteractiveAnnotation),
            "lsp:diagnostics_panel" => Self::Editor(EditorAction::LspDiagnosticsPanel),

            s if s.contains(':') && !s.starts_with("editor:") => Self::Buffer(s.to_string()),
            _ => Self::Noop,
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
