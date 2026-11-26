//! Editor actions, including movements and operations
use crate::text_objects::TextObjectSpec;

pub(crate) mod table;

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
    ChangeToLineEnd,
    DeleteToLineEnd,
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
        subcommand: crate::command_line::commands::types::SplitSubcommand,
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
    /// Replace the selected characters with a resolved Visual `r` argument.
    ReplaceSelection(char),
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
    /// Second `g` of `sgg` while an `sg` add-surround is in progress
    SurroundGiveLine,
    /// `v`: enter/resume charwise Visual selection.
    EnterVisualChar,
    /// `V`: enter/resume linewise Visual selection.
    EnterVisualLine,
    /// `Ctrl-V`: enter/resume blockwise Visual selection.
    EnterVisualBlock,
    /// `o`/`O` in Visual: swap which end (anchor vs cursor) is active.
    VisualSwapEnds,
    /// Visual uppercase operator: apply to whole selected lines.
    VisualLineOp(OperatorType),
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
}

fn unknown_action(s: &str) -> RiftError {
    RiftError::new(
        crate::error::ErrorType::Parse,
        "UNKNOWN_ACTION",
        format!("unknown action: {s}"),
    )
}

impl FromStr for Action {
    type Err = RiftError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(action) = table::parse_table(s) {
            return Ok(Self::Editor(action));
        }
        if table::is_kind_action(s) {
            return Ok(Self::Buffer(s.to_string()));
        }
        match s.split_once(':') {
            Some((namespace, name))
                if !namespace.is_empty()
                    && !name.is_empty()
                    && !table::is_builtin_namespace(namespace) =>
            {
                Ok(Self::Buffer(s.to_string()))
            }
            _ => Err(unknown_action(s)),
        }
    }
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Editor(action) => action.fmt(f),
            Self::Buffer(id) => f.write_str(id),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
