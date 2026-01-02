#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCommand {
    /// Quit command
    Quit {
        bangs: usize,
    },
    /// Set command with option and optional value
    Set {
        option: String,
        value: Option<String>,
        bangs: usize,
    },
    /// Set local command
    SetLocal {
        option: String,
        value: Option<String>,
        bangs: usize,
    },
    /// Write command (save file)
    Write {
        path: Option<String>,
        bangs: usize,
    },
    /// Write and quit command
    WriteQuit {
        path: Option<String>,
        bangs: usize,
    },
    /// Unknown command
    Unknown {
        name: String,
    },
    /// Ambiguous command (multiple matches)
    Ambiguous {
        prefix: String,
        matches: Vec<String>,
    },
    /// Notify command
    Notify {
        kind: String,
        message: String,
        bangs: usize,
    },
    /// Redraw the screen
    Redraw {
        bangs: usize,
    },
    /// Edit command (open file)
    Edit {
        path: Option<String>,
        bangs: usize,
    },
    /// Switch to next buffer
    BufferNext {
        bangs: usize,
    },
    /// Switch to previous buffer
    BufferPrevious {
        bangs: usize,
    },
    /// Substitute text
    Substitute {
        pattern: String,
        replacement: String,
        flags: String,
        range: Option<String>,
        bangs: usize,
    },
    /// Clear search highlights
    NoHighlight {
        bangs: usize,
    },
    BufferList,
    /// [TEMPORARY] Test split view (remove after manual verification)
    TestSelectView {
        bangs: usize,
    },
    /// Undo command (undo last change, optionally with count)
    Undo {
        count: Option<u64>,
        bangs: usize,
    },
    /// Redo command (redo last undone change, optionally with count)
    Redo {
        count: Option<u64>,
        bangs: usize,
    },
    /// Undo goto sequence (jump to specific edit sequence)
    UndoGoto {
        seq: u64,
        bangs: usize,
    },
    /// Create checkpoint at current position
    Checkpoint {
        bangs: usize,
    },
}
