//! Gap buffer implementation
//! Text storage with cursor always positioned at the gap

const std = @import("std");

pub const GapBuffer = struct {
    data: []u8,
    gap_start: usize,
    gap_end: usize,
    allocator: std.mem.Allocator,
    initialized: bool = true, // Track if buffer was properly initialized

    const INITIAL_CAPACITY = 1024;
    const MIN_GAP_SIZE = 16;

    pub fn init(allocator: std.mem.Allocator) !GapBuffer {
        const capacity = INITIAL_CAPACITY;
        const data = try allocator.alloc(u8, capacity);
        const gap_start: usize = 0;
        const gap_end: usize = capacity;

        return GapBuffer{
            .data = data,
            .gap_start = gap_start,
            .gap_end = gap_end,
            .allocator = allocator,
            .initialized = true,
        };
    }

    pub fn deinit(self: *GapBuffer) void {
        // Free the buffer data only if it was properly initialized
        // Check both initialized flag and that we have a valid allocation
        if (self.initialized and self.data.len > 0) {
            self.allocator.free(self.data);
        }
        self.* = undefined;
    }

    /// Insert bytes at the current cursor position (gap position)
    pub fn insert(self: *GapBuffer, bytes: []const u8) !void {
        if (bytes.len == 0) return;

        // Ensure gap is large enough
        const gap_size = self.gap_end - self.gap_start;
        if (gap_size < bytes.len) {
            try self.expandGap(bytes.len);
        }

        // Copy bytes into gap
        @memcpy(self.data[self.gap_start..self.gap_start + bytes.len], bytes);
        self.gap_start += bytes.len;
    }

    /// Delete the byte before the cursor (before gap)
    pub fn delete(self: *GapBuffer) bool {
        if (self.gap_start == 0) return false;
        self.gap_start -= 1;
        return true;
    }

    /// Move cursor left (move gap left)
    pub fn moveLeft(self: *GapBuffer) bool {
        if (self.gap_start == 0) return false;
        self.gap_start -= 1;
        self.gap_end -= 1;
        self.data[self.gap_end] = self.data[self.gap_start];
        return true;
    }

    /// Move cursor right (move gap right)
    pub fn moveRight(self: *GapBuffer) bool {
        if (self.gap_end >= self.data.len) return false;
        self.data[self.gap_start] = self.data[self.gap_end];
        self.gap_start += 1;
        self.gap_end += 1;
        return true;
    }

    /// Move cursor up (to previous line)
    pub fn moveUp(self: *GapBuffer) bool {
        const line_start = self.getLineStart(self.gap_start);
        if (line_start == 0) return false; // Already at first line

        // Find start of previous line
        const prev_line_start = self.getLineStart(line_start - 1);
        const current_col = self.gap_start - line_start;
        const prev_line_end = line_start - 1; // Before the newline
        const prev_line_len = prev_line_end - prev_line_start;

        // Calculate target column (don't go past end of previous line)
        const target_col = std.math.min(current_col, prev_line_len);
        const target_pos = prev_line_start + target_col;

        // Move gap to target position
        self.moveGapTo(target_pos);
        return true;
    }

    /// Move cursor down (to next line)
    pub fn moveDown(self: *GapBuffer) bool {
        const line_start = self.getLineStart(self.gap_start);
        const line_end = self.getLineEnd(self.gap_start);
        const current_col = self.gap_start - line_start;

        // Check if there's a next line
        if (line_end >= self.getContentLength()) return false;

        // Find start of next line
        const next_line_start = line_end + 1; // After the newline
        const next_line_end = self.getLineEnd(next_line_start);
        const next_line_len = next_line_end - next_line_start;

        // Calculate target column (don't go past end of next line)
        const target_col = std.math.min(current_col, next_line_len);
        const target_pos = next_line_start + target_col;

        // Move gap to target position
        self.moveGapTo(target_pos);
        return true;
    }

    /// Move cursor to start of line
    pub fn moveToLineStart(self: *GapBuffer) void {
        const line_start = self.getLineStart(self.gap_start);
        self.moveGapTo(line_start);
    }

    /// Move cursor to end of line
    pub fn moveToLineEnd(self: *GapBuffer) void {
        const line_end = self.getLineEnd(self.gap_start);
        self.moveGapTo(line_end);
    }

    /// Move cursor to start of buffer
    pub fn moveToBufferStart(self: *GapBuffer) void {
        self.moveGapTo(0);
    }

    /// Move cursor to end of buffer
    pub fn moveToBufferEnd(self: *GapBuffer) void {
        const content_len = self.getContentLength();
        self.moveGapTo(content_len);
    }

    /// Get current line number (0-indexed)
    pub fn getLine(self: *const GapBuffer) usize {
        var line: usize = 0;
        var pos: usize = 0;
        const before_gap = self.data[0..self.gap_start];
        for (before_gap) |byte| {
            if (byte == '\n') {
                line += 1;
            }
            pos += 1;
        }
        return line;
    }

    /// Get total number of lines
    pub fn getTotalLines(self: *const GapBuffer) usize {
        var lines: usize = 1; // At least one line
        const before_gap = self.data[0..self.gap_start];
        const after_gap = self.data[self.gap_end..];
        for (before_gap) |byte| {
            if (byte == '\n') lines += 1;
        }
        for (after_gap) |byte| {
            if (byte == '\n') lines += 1;
        }
        return lines;
    }

    /// Get start position of line containing pos
    fn getLineStart(self: *const GapBuffer, pos: usize) usize {
        var line_start: usize = 0;
        var current_pos: usize = 0;
        const before_gap = self.data[0..self.gap_start];
        const after_gap = self.data[self.gap_end..];

        // Search through before_gap
        for (before_gap) |byte| {
            if (current_pos == pos) return line_start;
            if (byte == '\n') {
                line_start = current_pos + 1;
            }
            current_pos += 1;
        }

        // If pos is at gap, return current line_start
        if (current_pos == pos) return line_start;

        // Search through after_gap
        for (after_gap) |byte| {
            if (current_pos == pos) return line_start;
            if (byte == '\n') {
                line_start = current_pos + 1;
            }
            current_pos += 1;
        }

        return line_start;
    }

    /// Get end position of line containing pos (position of newline or end of buffer)
    fn getLineEnd(self: *const GapBuffer, pos: usize) usize {
        var current_pos: usize = 0;
        const before_gap = self.data[0..self.gap_start];
        const after_gap = self.data[self.gap_end..];
        const content_len = self.getContentLength();

        // Search through before_gap
        for (before_gap) |byte| {
            if (current_pos > pos and byte == '\n') {
                return current_pos;
            }
            current_pos += 1;
        }

        // Search through after_gap
        for (after_gap) |byte| {
            if (current_pos > pos and byte == '\n') {
                return current_pos;
            }
            current_pos += 1;
        }

        // If no newline found, return end of content
        return content_len;
    }

    /// Get content as a single line (for current line)
    pub fn getLine(self: *const GapBuffer) []const u8 {
        const line_start = self.getLineStart(self.gap_start);
        const line_end = self.getLineEnd(self.gap_start);
        // This is a simplified version - full implementation would need to reconstruct the line
        // For now, return empty slice - rendering will handle this differently
        _ = line_start;
        _ = line_end;
        return "";
    }

    /// Delete current line (dd command)
    pub fn deleteLine(self: *GapBuffer) bool {
        const line_start = self.getLineStart(self.gap_start);
        const line_end = self.getLineEnd(self.gap_start);

        if (line_start == line_end) return false; // Empty line

        // Move gap to line_start
        self.moveGapTo(line_start);

        // Delete from line_start to line_end (including newline if present)
        const delete_len = line_end - line_start;
        if (line_end < self.getContentLength()) {
            // Include newline
            self.gap_end += delete_len + 1;
        } else {
            // No newline at end
            self.gap_end += delete_len;
        }

        return true;
    }

    /// Move gap to specified position
    fn moveGapTo(self: *GapBuffer, target_pos: usize) void {
        const current_pos = self.gap_start;
        if (target_pos == current_pos) return;

        if (target_pos < current_pos) {
            // Move gap left - move bytes from before gap to after gap
            const move_len = current_pos - target_pos;
            var i: usize = 0;
            while (i < move_len) : (i += 1) {
                self.gap_start -= 1;
                self.gap_end -= 1;
                self.data[self.gap_end] = self.data[self.gap_start];
            }
        } else {
            // Move gap right - move bytes from after gap to before gap
            const move_len = target_pos - current_pos;
            var i: usize = 0;
            while (i < move_len) : (i += 1) {
                self.data[self.gap_start] = self.data[self.gap_end];
                self.gap_start += 1;
                self.gap_end += 1;
            }
        }
    }

    /// Get total content length (excluding gap)
    fn getContentLength(self: *const GapBuffer) usize {
        return self.gap_start + (self.data.len - self.gap_end);
    }

    /// Expand gap to accommodate at least min_size bytes
    fn expandGap(self: *GapBuffer, min_size: usize) !void {
        const current_gap_size = self.gap_end - self.gap_start;
        const needed_size = min_size - current_gap_size;
        if (needed_size <= 0) return;

        // Calculate new capacity (at least double, or enough for min_size)
        const current_capacity = self.data.len;
        const content_size = current_capacity - current_gap_size;
        const min_capacity = content_size + min_size;
        const new_capacity = std.math.max(current_capacity * 2, min_capacity);

        // Allocate new buffer
        const old_data = self.data;
        const new_data = try self.allocator.alloc(u8, new_capacity);

        // Copy before gap
        @memcpy(new_data[0..self.gap_start], old_data[0..self.gap_start]);

        // Copy after gap to new position
        const after_gap_len = old_data.len - self.gap_end;
        const new_gap_end = new_capacity - after_gap_len;
        @memcpy(new_data[new_gap_end..], old_data[self.gap_end..]);

        // Update state
        self.data = new_data;
        self.gap_end = new_gap_end;

        // Free old buffer
        self.allocator.free(old_data);
    }

    /// Get all content as a slice (for rendering)
    /// Returns a slice that represents the content without the gap
    pub fn getContent(self: *const GapBuffer) []const u8 {
        const before_gap = self.data[0..self.gap_start];
        // We can't return a contiguous slice, so callers need to handle this differently
        // For now, return before_gap only - rendering will need special handling
        _ = self.data[self.gap_end..]; // after_gap exists but not returned
        return before_gap;
    }

    /// Get content before gap
    pub fn getBeforeGap(self: *const GapBuffer) []const u8 {
        return self.data[0..self.gap_start];
    }

    /// Get content after gap
    pub fn getAfterGap(self: *const GapBuffer) []const u8 {
        return self.data[self.gap_end..];
    }

    /// Load file content into buffer
    pub fn loadFromFile(self: *GapBuffer, file_path: []const u8) !void {
        const file = try std.fs.cwd().openFile(file_path, .{});
        defer file.close();

        const file_size = try file.getEndPos();
        const content = try file.readToEndAlloc(self.allocator, file_size);
        defer self.allocator.free(content);

        // Clear buffer first
        self.gap_start = 0;
        self.gap_end = self.data.len;

        // Ensure capacity - if we need to reallocate, do it safely
        if (self.data.len < content.len) {
            const old_data = self.data;
            const new_capacity = if (content.len * 2 > INITIAL_CAPACITY) content.len * 2 else INITIAL_CAPACITY;
            self.data = try self.allocator.alloc(u8, new_capacity);
            self.gap_end = new_capacity;
            // Free old data only after successful allocation
            self.allocator.free(old_data);
        }

        // Insert content
        try self.insert(content);
    }
};

