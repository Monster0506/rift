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
    /// LSP rename dialog (entered with `<Space>rn`, prompt char @)
    Rename,
    /// Replace mode (entered with R): each char overwrites instead of inserting
    Replace,
    /// Charwise visual selection (`v`).
    Visual,
    /// Linewise visual selection (`V`).
    VisualLine,
    /// Rectangular visual selection (`Ctrl-V`).
    VisualBlock,
}

impl Mode {
    /// Returns the lowercase mode name for Lua state. `OperatorPending` uses `"normal"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal | Self::OperatorPending => "normal",
            Self::Insert => "insert",
            Self::Command => "command",
            Self::Search => "search",
            Self::Rename => "rename",
            Self::Replace => "replace",
            Self::Visual | Self::VisualLine | Self::VisualBlock => "visual",
        }
    }

    /// True for any of the three Visual-family modes.
    #[must_use]
    pub const fn is_visual(self) -> bool {
        matches!(self, Self::Visual | Self::VisualLine | Self::VisualBlock)
    }

    /// The `RangeKind` a region built from this mode should use, or `None`
    /// for non-visual modes.
    #[must_use]
    pub const fn visual_range_kind(self) -> Option<crate::wrap::RangeKind> {
        match self {
            Self::Visual => Some(crate::wrap::RangeKind::Charwise),
            Self::VisualLine => Some(crate::wrap::RangeKind::Linewise),
            Self::VisualBlock => Some(crate::wrap::RangeKind::Blockwise),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
