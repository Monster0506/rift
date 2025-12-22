//! ANSI escape sequence constants and fallback functions
//! These are used as fallbacks when platform-specific APIs are not available

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

/// Reverse video (invert colors)
pub const REVERSE_VIDEO = ESC ++ "[7m";

/// Reset all attributes
pub const RESET_ATTRIBUTES = ESC ++ "[0m";

/// Format cursor position escape sequence (1-indexed)
pub fn formatCursorPosition(buf: []u8, row: u16, col: u16) ![]const u8 {
    const std = @import("std");
    return std.fmt.bufPrint(buf, ESC ++ "[{d};{d}H", .{ row + 1, col + 1 });
}

/// Write ANSI sequence to clear screen (fallback)
pub fn writeClearScreen(writeFn: *const fn (bytes: []const u8) anyerror!void) !void {
    try writeFn(CLEAR_SCREEN);
    try writeFn(RESET_CURSOR);
}

/// Write ANSI sequence to move cursor (fallback)
pub fn writeMoveCursor(writeFn: *const fn (bytes: []const u8) anyerror!void, buf: []u8, row: u16, col: u16) !void {
    const seq = try formatCursorPosition(buf, row, col);
    try writeFn(seq);
}

/// Write ANSI sequence to hide cursor (fallback)
pub fn writeHideCursor(writeFn: *const fn (bytes: []const u8) anyerror!void) !void {
    try writeFn(HIDE_CURSOR);
}

/// Write ANSI sequence to show cursor (fallback)
pub fn writeShowCursor(writeFn: *const fn (bytes: []const u8) anyerror!void) !void {
    try writeFn(SHOW_CURSOR);
}

/// Write ANSI sequence to clear to end of line (fallback)
pub fn writeClearToEndOfLine(writeFn: *const fn (bytes: []const u8) anyerror!void) !void {
    try writeFn(CLEAR_TO_EOL);
}

