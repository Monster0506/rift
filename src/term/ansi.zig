//! ANSI escape sequence constants

/// ANSI escape sequence prefix
pub const ESC = "\x1b";

/// Clear entire screen
pub const CLEAR_SCREEN = ESC ++ "[2J";

/// Clear from cursor to end of screen
pub const CLEAR_TO_END = ESC ++ "[0J";

/// Clear from cursor to end of line
pub const CLEAR_TO_EOL = ESC ++ "[K";

/// Reset cursor position to top-left
pub const RESET_CURSOR = ESC ++ "[H";

/// Hide cursor
pub const HIDE_CURSOR = ESC ++ "[?25l";

/// Show cursor
pub const SHOW_CURSOR = ESC ++ "[?25h";

/// Format cursor position escape sequence (1-indexed)
pub fn formatCursorPosition(buf: []u8, row: u16, col: u16) ![]const u8 {
    const std = @import("std");
    return std.fmt.bufPrint(buf, ESC ++ "[{d};{d}H", .{ row + 1, col + 1 });
}

