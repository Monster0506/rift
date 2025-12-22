//! ANSI escape sequences for terminal control
//! Used as fallback when platform-specific APIs are unavailable

/// ANSI escape sequence constants
pub const CLEAR_SCREEN: &str = "\x1b[2J";
pub const RESET_CURSOR: &str = "\x1b[H";
pub const HIDE_CURSOR: &str = "\x1b[?25l";
pub const SHOW_CURSOR: &str = "\x1b[?25h";
pub const CLEAR_TO_EOL: &str = "\x1b[K";
pub const REVERSE_VIDEO: &str = "\x1b[7m";
pub const RESET_ATTRIBUTES: &str = "\x1b[0m";

/// Format cursor position escape sequence
pub fn format_cursor_position(row: u16, col: u16) -> String {
    format!("\x1b[{};{}H", row + 1, col + 1)
}

/// Write clear screen sequence
pub fn write_clear_screen(write_fn: &mut dyn FnMut(&[u8]) -> Result<(), String>) -> Result<(), String> {
    write_fn(CLEAR_SCREEN.as_bytes())?;
    write_fn(RESET_CURSOR.as_bytes())
}

/// Write move cursor sequence
pub fn write_move_cursor(write_fn: &mut dyn FnMut(&[u8]) -> Result<(), String>, row: u16, col: u16) -> Result<(), String> {
    let seq = format_cursor_position(row, col);
    write_fn(seq.as_bytes())
}

/// Write hide cursor sequence
pub fn write_hide_cursor(write_fn: &mut dyn FnMut(&[u8]) -> Result<(), String>) -> Result<(), String> {
    write_fn(HIDE_CURSOR.as_bytes())
}

/// Write show cursor sequence
pub fn write_show_cursor(write_fn: &mut dyn FnMut(&[u8]) -> Result<(), String>) -> Result<(), String> {
    write_fn(SHOW_CURSOR.as_bytes())
}

/// Write clear to end of line sequence
pub fn write_clear_to_end_of_line(write_fn: &mut dyn FnMut(&[u8]) -> Result<(), String>) -> Result<(), String> {
    write_fn(CLEAR_TO_EOL.as_bytes())
}

