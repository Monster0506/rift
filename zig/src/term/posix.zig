//! POSIX terminal backend (stub)
//! Uses termios for raw mode and ANSI escape sequences

const std = @import("std");
const backend = @import("backend.zig");
const ansi = @import("ansi.zig");
const Key = @import("../key.zig").Key;

pub const PosixTerminal = struct {
    terminal: backend.Terminal,
    // TODO: Add POSIX-specific fields (termios, etc.)

    pub fn init(allocator: std.mem.Allocator) !PosixTerminal {
        _ = allocator;
        return error.NotImplemented;
    }

    fn initImpl(ctx: *anyopaque) !void {
        _ = ctx;
        return error.NotImplemented;
    }

    fn deinitImpl(ctx: *anyopaque) void {
        _ = ctx;
    }

    fn readKeyImpl(ctx: *anyopaque) !Key {
        _ = ctx;
        return error.NotImplemented;
    }

    fn writeImpl(ctx: *anyopaque, bytes: []const u8) !void {
        _ = ctx;
        _ = bytes;
        return error.NotImplemented;
    }

    fn getSizeImpl(ctx: *anyopaque) !backend.Size {
        _ = ctx;
        return error.NotImplemented;
    }

    fn clearScreenImpl(ctx: *anyopaque) !void {
        _ = ctx;
        return error.NotImplemented;
    }

    fn moveCursorImpl(ctx: *anyopaque, row: u16, col: u16) !void {
        _ = ctx;
        _ = row;
        _ = col;
        return error.NotImplemented;
    }

    fn hideCursorImpl(ctx: *anyopaque) !void {
        _ = ctx;
        return error.NotImplemented;
    }

    fn showCursorImpl(ctx: *anyopaque) !void {
        _ = ctx;
        return error.NotImplemented;
    }

    fn clearToEndOfLineImpl(ctx: *anyopaque) !void {
        _ = ctx;
        return error.NotImplemented;
    }
};

