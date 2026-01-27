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
    /// Overlay mode (split-view overlays like :undotree)
    Overlay,
    /// Operator pending mode (e.g. after pressing 'd')
    OperatorPending,
}
