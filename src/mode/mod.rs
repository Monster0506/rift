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
    /// LSP rename dialog (entered with <Space>rn, prompt char @)
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
    /// Canonical lowercase name, e.g. for Lua plugin state.
    /// `OperatorPending` reports as `"normal"` since plugins see no distinction.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Normal | Mode::OperatorPending => "normal",
            Mode::Insert => "insert",
            Mode::Command => "command",
            Mode::Search => "search",
            Mode::Rename => "rename",
            Mode::Replace => "replace",
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock => "visual",
        }
    }

    /// True for any of the three Visual-family modes.
    pub fn is_visual(self) -> bool {
        matches!(self, Mode::Visual | Mode::VisualLine | Mode::VisualBlock)
    }
}

#[cfg(test)]
mod tests {
    use super::Mode;

    #[test]
    fn visual_variants_report_as_visual_string() {
        assert_eq!(Mode::Visual.as_str(), "visual");
        assert_eq!(Mode::VisualLine.as_str(), "visual");
        assert_eq!(Mode::VisualBlock.as_str(), "visual");
    }

    #[test]
    fn is_visual_true_only_for_visual_variants() {
        assert!(Mode::Visual.is_visual());
        assert!(Mode::VisualLine.is_visual());
        assert!(Mode::VisualBlock.is_visual());
        assert!(!Mode::Normal.is_visual());
        assert!(!Mode::OperatorPending.is_visual());
        assert!(!Mode::Insert.is_visual());
    }
}
