//! Key input decoding
//! Translates raw terminal input into logical keys

/// Logical key representation
pub const Key = union(enum) {
    /// Printable character
    char: u8,
    /// Backspace key
    backspace,
    /// Enter/Return key
    enter,
    /// Escape key
    escape,
    /// Arrow keys
    arrow_up,
    arrow_down,
    arrow_left,
    arrow_right,
    /// Home key
    home,
    /// End key
    end,
    /// Page Up
    page_up,
    /// Page Down
    page_down,
    /// Delete key
    delete,
    /// Control key combinations (e.g., Ctrl+C)
    ctrl: u8,
};

