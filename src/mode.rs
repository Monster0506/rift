//! Editor mode definitions

/// Editor operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Normal mode (command mode)
    Normal,
    /// Insert mode (text editing)
    Insert,
}

