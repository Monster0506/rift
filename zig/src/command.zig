//! Command dispatch and keybindings
//! Translates keys into editor commands based on current mode

const std = @import("std");
const Key = @import("key.zig").Key;
const Mode = @import("mode.zig").Mode;
const buffer = @import("buffer.zig");

/// Editor commands
pub const Command = enum {
    // Movement
    move_left,
    move_right,
    move_up,
    move_down,
    move_to_line_start,
    move_to_line_end,
    move_to_buffer_start,
    move_to_buffer_end,

    // Editing
    enter_insert_mode,
    enter_insert_mode_after,
    delete_char,
    delete_line,
    insert_char,
    insert_newline,
    backspace,

    // Control
    quit,
    noop,
};

/// Command dispatcher state
pub const Dispatcher = struct {
    mode: Mode,
    pending_key: ?Key = null,

    pub fn init(mode: Mode) Dispatcher {
        return Dispatcher{ .mode = mode };
    }

    /// Translate a key into a command based on current mode
    pub fn translateKey(self: *Dispatcher, key: Key) Command {
        switch (self.mode) {
            .normal => return self.translateNormalMode(key),
            .insert => return self.translateInsertMode(key),
        }
    }

    fn translateNormalMode(self: *Dispatcher, key: Key) Command {
        // Handle multi-key sequences
        if (self.pending_key) |pending| {
            self.pending_key = null;
            return self.handleNormalModeSequence(pending, key);
        }

        switch (key) {
            .char => |ch| {
                switch (ch) {
                    'h' => return .move_left,
                    'j' => return .move_down,
                    'k' => return .move_up,
                    'l' => return .move_right,
                    '0' => return .move_to_line_start,
                    '$' => return .move_to_line_end,
                    'i' => return .enter_insert_mode,
                    'a' => return .enter_insert_mode_after,
                    'x' => return .delete_char,
                    'q' => return .quit,
                    'd' => {
                        // Start sequence for 'dd'
                        self.pending_key = key;
                        return .noop;
                    },
                    'g' => {
                        // Start sequence for 'gg'
                        self.pending_key = key;
                        return .noop;
                    },
                    'G' => return .move_to_buffer_end,
                    else => return .noop,
                }
            },
            .arrow_left => return .move_left,
            .arrow_right => return .move_right,
            .arrow_up => return .move_up,
            .arrow_down => return .move_down,
            .home => return .move_to_line_start,
            .end => return .move_to_line_end,
            else => return .noop,
        }
    }

    fn handleNormalModeSequence(self: *Dispatcher, first: Key, second: Key) Command {
        _ = self;
        switch (first) {
            .char => |ch1| {
                switch (ch1) {
                    'd' => {
                        if (second == .char and second.char == 'd') {
                            return .delete_line;
                        }
                    },
                    'g' => {
                        if (second == .char and second.char == 'g') {
                            return .move_to_buffer_start;
                        }
                    },
                    else => {},
                }
            },
            else => {},
        }
        return .noop;
    }

    fn translateInsertMode(self: *Dispatcher, key: Key) Command {
        _ = self;
        switch (key) {
            .char => |ch| {
                // Handle printable characters (including Tab which is 9)
                if (ch == 9) {
                    // Tab - insert tab character
                    return .insert_char;
                }
                if (ch >= 32 and ch < 127) {
                    return .insert_char;
                }
                return .noop;
            },
            .ctrl => |_| {
                // Handle Ctrl key combinations in insert mode
                // For now, we'll insert them as characters (Ctrl+A = 0x01, etc.)
                // This allows Ctrl keys to be inserted into the buffer
                return .insert_char;
            },
            .backspace => return .backspace,
            .enter => return .insert_newline,
            .escape => return .enter_insert_mode, // Exit insert mode (returns to normal)
            else => return .noop,
        }
    }
};

/// Execute a command on the editor buffer
pub fn executeCommand(cmd: Command, buf: *buffer.GapBuffer, key: ?Key) void {
    switch (cmd) {
        .move_left => _ = buf.moveLeft(),
        .move_right => _ = buf.moveRight(),
        .move_up => _ = buf.moveUp(),
        .move_down => _ = buf.moveDown(),
        .move_to_line_start => buf.moveToLineStart(),
        .move_to_line_end => buf.moveToLineEnd(),
        .move_to_buffer_start => buf.moveToStart(),
        .move_to_buffer_end => buf.moveToEnd(),
        .delete_char => _ = buf.delete(),
        .delete_line => _ = buf.deleteLine(),
        .insert_char => {
            if (key) |k| {
                switch (k) {
                    .char => |ch| {
                        buf.insert(&[_]u8{ch}) catch {};
                    },
                    .ctrl => |ch| {
                        // Insert Ctrl character (0x01-0x1A for Ctrl+A-Z, 0x1D for Ctrl+], etc.)
                        const ctrl_char: u8 = if (ch >= 'a' and ch <= 'z') 
                            @as(u8, @intCast(ch - 'a' + 1))
                        else if (ch == ']')
                            29  // Ctrl+] = 0x1D
                        else
                            ch; // Fallback
                        buf.insert(&[_]u8{ctrl_char}) catch {};
                    },
                    else => {},
                }
            }
        },
        .insert_newline => {
            buf.insert("\n") catch {};
        },
        .backspace => _ = buf.delete(),
        .enter_insert_mode => {},
        .enter_insert_mode_after => {
            _ = buf.moveRight();
        },
        .quit => {},
        .noop => {},
    }
}

