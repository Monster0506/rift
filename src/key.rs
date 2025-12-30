//! Key representation for editor input

/// Represents a key press event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// Printable character
    Char(char),
    /// Control key combination (e.g., Ctrl+A)
    Ctrl(u8),
    /// Arrow keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    CtrlArrowUp,
    CtrlArrowDown,
    CtrlArrowLeft,
    CtrlArrowRight,
    /// Navigation keys
    Home,
    End,
    PageUp,
    PageDown,
    /// Editing keys
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    /// System events
    Resize(u16, u16),
}
