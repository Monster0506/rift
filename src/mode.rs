//! Editor mode definitions

/// Editor operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Normal mode (command mode)
    Normal,
    /// Insert mode (text editing)
    Insert,
    /// Command mode (ex command line, entered with :)
    Command,
    /// Search mode (entered with /)
    Search,
    /// Operator pending mode (e.g. after pressing 'd')
    OperatorPending,
}

impl Mode {
    /// Canonical lowercase name, e.g. for Lua plugin state.
    /// `OperatorPending` reports as `"normal"` since plugins see no distinction.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Normal | Mode::OperatorPending => "normal",
            Mode::Insert => "insert",
            Mode::Command => "command",
            Mode::Search => "search",
        }
    }
}
