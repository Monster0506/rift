//! Terminal abstraction interface
//! Defines the contract that all terminal backends must implement

const std = @import("std");
const Key = @import("../key.zig").Key;

/// Terminal size information
pub const Size = struct {
    rows: u16,
    cols: u16,
};

/// Terminal interface contract
/// All terminal backends must implement these operations
pub const Terminal = struct {
    /// Context pointer - points to the concrete terminal implementation
    ctx: *anyopaque,

    /// Initialize terminal and enter raw mode
    /// Must be called before any other operations
    initFn: *const fn (ctx: *anyopaque) anyerror!void,

    /// Restore terminal to original state
    /// Must be called before program exit
    deinitFn: *const fn (ctx: *anyopaque) void,

    /// Read and decode a single keypress
    /// Blocks until a key is available
    readKeyFn: *const fn (ctx: *anyopaque) anyerror!Key,

    /// Write bytes to stdout
    writeFn: *const fn (ctx: *anyopaque, bytes: []const u8) anyerror!void,

    /// Get terminal dimensions
    getSizeFn: *const fn (ctx: *anyopaque) anyerror!Size,

    /// Clear entire screen
    clearScreenFn: *const fn (ctx: *anyopaque) anyerror!void,

    /// Move cursor to specified position (0-indexed)
    moveCursorFn: *const fn (ctx: *anyopaque, row: u16, col: u16) anyerror!void,

    /// Hide cursor
    hideCursorFn: *const fn (ctx: *anyopaque) anyerror!void,

    /// Show cursor
    showCursorFn: *const fn (ctx: *anyopaque) anyerror!void,

    /// Initialize terminal and enter raw mode
    pub fn init(self: *Terminal) !void {
        try self.initFn(self.ctx);
    }

    /// Restore terminal to original state
    pub fn deinit(self: *Terminal) void {
        self.deinitFn(self.ctx);
    }

    /// Read and decode a single keypress
    pub fn readKey(self: *Terminal) !Key {
        return try self.readKeyFn(self.ctx);
    }

    /// Write bytes to stdout
    pub fn write(self: *Terminal, bytes: []const u8) !void {
        try self.writeFn(self.ctx, bytes);
    }

    /// Get terminal dimensions
    pub fn getSize(self: *Terminal) !Size {
        return try self.getSizeFn(self.ctx);
    }

    /// Clear entire screen
    pub fn clearScreen(self: *Terminal) !void {
        try self.clearScreenFn(self.ctx);
    }

    /// Move cursor to specified position (0-indexed)
    pub fn moveCursor(self: *Terminal, row: u16, col: u16) !void {
        try self.moveCursorFn(self.ctx, row, col);
    }

    /// Hide cursor
    pub fn hideCursor(self: *Terminal) !void {
        try self.hideCursorFn(self.ctx);
    }

    /// Show cursor
    pub fn showCursor(self: *Terminal) !void {
        try self.showCursorFn(self.ctx);
    }
};

