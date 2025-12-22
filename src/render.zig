//! Screen rendering
//! Full-screen redraw with viewport management

const std = @import("std");
const buffer = @import("buffer.zig");
const backend = @import("term/backend.zig");
const mode = @import("mode.zig");
const key = @import("key.zig");

/// Viewport tracks visible lines
pub const Viewport = struct {
    top_line: usize,
    visible_rows: u16,
    visible_cols: u16,

    pub fn init(visible_rows: u16, visible_cols: u16) Viewport {
        return Viewport{
            .top_line = 0,
            .visible_rows = visible_rows,
            .visible_cols = visible_cols,
        };
    }

    /// Update viewport to ensure cursor line is visible
    pub fn update(self: *Viewport, cursor_line: usize, total_lines: usize) void {
        // If cursor is above viewport, scroll up
        if (cursor_line < self.top_line) {
            self.top_line = cursor_line;
        }

        // If cursor is below viewport, scroll down
        const bottom_line = self.top_line + self.visible_rows - 1;
        if (cursor_line > bottom_line) {
            self.top_line = cursor_line - (self.visible_rows - 1);
            if (self.top_line > total_lines) {
                self.top_line = if (total_lines > 0) total_lines - 1 else 0;
            }
        }

        // Ensure top_line doesn't go negative
        if (self.top_line > total_lines) {
            self.top_line = if (total_lines > 0) total_lines - 1 else 0;
        }
    }
};

/// Render the entire screen
pub fn render(
    term: *backend.Terminal,
    buf: *buffer.GapBuffer,
    viewport: *Viewport,
    allocator: std.mem.Allocator,
    current_mode: mode.Mode,
    pending_key: ?key.Key,
) !void {
    // Clear screen
    try term.clearScreen();

    // Update viewport
    const cursor_line = buf.getLine();
    const total_lines = buf.getTotalLines();
    viewport.update(cursor_line, total_lines);

    const before_gap = buf.getBeforeGap();
    const after_gap = buf.getAfterGap();

    // Render visible lines by iterating through content
    // Reserve last row for status bar
    const content_rows = viewport.visible_rows - 1;
    const start_line = viewport.top_line;
    var line_num: usize = 0;
    var display_row: u16 = 0; // Tracks which visible row we're rendering (0 to content_rows-1)
    var line_buffer = std.ArrayList(u8).empty;
    defer line_buffer.deinit(allocator);

    // Iterate through content to find and render lines
    var before_pos: usize = 0;
    var after_pos: usize = 0;
    var in_before_gap = true;

    while (line_num < start_line + content_rows) {
        line_buffer.clearRetainingCapacity();

        // Read one line
        var line_complete = false;
        while (!line_complete) {
            var byte: u8 = undefined;
            var at_end = false;

            if (in_before_gap) {
                if (before_pos < before_gap.len) {
                    byte = before_gap[before_pos];
                    before_pos += 1;
                    if (byte == '\n') {
                        line_complete = true;
                    } else {
                        try line_buffer.append(allocator, byte);
                    }
                } else {
                    in_before_gap = false;
                    if (after_pos < after_gap.len) {
                        byte = after_gap[after_pos];
                        after_pos += 1;
                        if (byte == '\n') {
                            line_complete = true;
                        } else {
                            try line_buffer.append(allocator, byte);
                        }
                    } else {
                        at_end = true;
                        line_complete = true;
                    }
                }
            } else {
                if (after_pos < after_gap.len) {
                    byte = after_gap[after_pos];
                    after_pos += 1;
                    if (byte == '\n') {
                        line_complete = true;
                    } else {
                        try line_buffer.append(allocator, byte);
                    }
                } else {
                    at_end = true;
                    line_complete = true;
                }
            }

            if (at_end and line_buffer.items.len == 0) {
                // End of content
                break;
            }
        }

        // Render this line if it's in viewport
        if (line_num >= start_line) {
            // Check if we've rendered all visible content rows (excluding status bar)
            if (display_row >= content_rows) {
                break; // Past visible area
            }
            
            try term.moveCursor(display_row, 0);

            const line = line_buffer.items;
            const line_to_display = if (line.len > viewport.visible_cols) line[0..viewport.visible_cols] else line;
            try term.write(line_to_display);

            // Clear to end of line
            if (line.len < viewport.visible_cols) {
                try term.write("\x1b[K");
            }

            display_row += 1;
        }

        line_num += 1;

        // Stop if we've rendered all content and filled screen
        if (line_buffer.items.len == 0 and before_pos >= before_gap.len and after_pos >= after_gap.len) {
            break;
        }
    }

    // Clear remaining lines (except status bar)
    while (display_row < content_rows) {
        try term.moveCursor(display_row, 0);
        try term.write("\x1b[K");
        display_row += 1;
    }

    // Render status bar on last line
    try renderStatusBar(term, viewport, current_mode, pending_key);

    // Position cursor (accounting for status bar)
    const cursor_line_in_viewport = if (cursor_line >= viewport.top_line and cursor_line < start_line + content_rows) 
        cursor_line - viewport.top_line 
    else 
        0;
    const cursor_col = calculateCursorColumn(buf, cursor_line);
    const display_col = if (cursor_col < viewport.visible_cols - 1) cursor_col else viewport.visible_cols - 1;
    try term.moveCursor(@as(u16, @intCast(cursor_line_in_viewport)), @as(u16, @intCast(display_col)));
}

/// Render status bar at the bottom of the screen
fn renderStatusBar(
    term: *backend.Terminal,
    viewport: *Viewport,
    current_mode: mode.Mode,
    pending_key: ?key.Key,
) !void {
    const status_row = viewport.visible_rows - 1;
    try term.moveCursor(status_row, 0);
    
    // Invert colors for status bar (reverse video)
    try term.write("\x1b[7m");
    
    // Mode indicator
    const mode_str = switch (current_mode) {
        .normal => "NORMAL",
        .insert => "INSERT",
    };
    try term.write(mode_str);
    
    // Pending key indicator
    if (pending_key) |pending| {
        try term.write(" [");
        const key_str = switch (pending) {
            .char => |ch| blk: {
                var buf: [1]u8 = undefined;
                buf[0] = ch;
                break :blk buf[0..];
            },
            .backspace => "BACKSPACE",
            .enter => "ENTER",
            .escape => "ESC",
            .arrow_up => "UP",
            .arrow_down => "DOWN",
            .arrow_left => "LEFT",
            .arrow_right => "RIGHT",
            .home => "HOME",
            .end => "END",
            .page_up => "PGUP",
            .page_down => "PGDN",
            .delete => "DEL",
            .ctrl => |ch| blk: {
                var buf: [32]u8 = undefined;
                const len = std.fmt.bufPrint(&buf, "CTRL-{}", .{ch}) catch "CTRL-?";
                break :blk len;
            },
        };
        try term.write(key_str);
        try term.write("]");
    }
    
    // Fill rest of line with spaces
    const mode_len = mode_str.len;
    var used_cols = mode_len;
    if (pending_key) |pending| {
        // Calculate length of pending key string
        const key_str_len: usize = switch (pending) {
            .char => 1,
            .backspace => 9,
            .enter => 5,
            .escape => 3,
            .arrow_up => 2,
            .arrow_down => 2,
            .arrow_left => 4,
            .arrow_right => 5,
            .home => 4,
            .end => 3,
            .page_up => 4,
            .page_down => 4,
            .delete => 3,
            .ctrl => 6,
        };
        used_cols += 3 + key_str_len; // "[key]"
    }
    const remaining_cols = if (viewport.visible_cols > used_cols) viewport.visible_cols - used_cols else 0;
    
    var i: usize = 0;
    while (i < remaining_cols) : (i += 1) {
        try term.write(" ");
    }
    
    // Reset colors
    try term.write("\x1b[0m");
}

/// Calculate cursor column position
fn calculateCursorColumn(buf: *buffer.GapBuffer, line: usize) usize {
    const before_gap = buf.getBeforeGap();
    const after_gap = buf.getAfterGap();

    // Count lines and find column
    var current_line: usize = 0;
    var col: usize = 0;
    var pos: usize = 0;

    // Count through before_gap
    for (before_gap) |byte| {
        if (byte == '\n') {
            if (current_line == line) {
                return col;
            }
            current_line += 1;
            col = 0;
        } else {
            col += 1;
        }
        pos += 1;
    }

    // If we're at the gap position
    if (current_line == line) {
        return col;
    }

    // Count through after_gap
    for (after_gap) |byte| {
        if (byte == '\n') {
            if (current_line == line) {
                return col;
            }
            current_line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    return col;
}

