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

    /// Move cursor up one line
    pub fn moveUp(self: *GapBuffer) bool {
        const line_start = self.getLineStart(self.gap_start);
        if (line_start == 0) return false; // Already at first line

        // Find start of previous line
        var prev_line_start = line_start - 1;
        while (prev_line_start > 0 and self.data[prev_line_start - 1] != '\n') {
            prev_line_start -= 1;
        }

        // Calculate column position in current line
        const current_col = self.gap_start - line_start;

        // Find target position in previous line
        var target_pos = prev_line_start;
        var col_count: usize = 0;
        while (target_pos < line_start and col_count < current_col) {
            if (self.data[target_pos] == '\n') break;
            target_pos += 1;
            col_count += 1;
        }

        // Move gap to target position
        return self.moveGapTo(target_pos);
    }

    /// Move cursor down one line
    pub fn moveDown(self: *GapBuffer) bool {
        const line_end = self.getLineEnd(self.gap_start);
        const content_end = self.getContentEnd();
        if (line_end >= content_end) return false; // Already at last line

        // Find start of next line
        const next_line_start = line_end + 1;
        if (next_line_start >= self.getContentEnd()) return false;

        // Calculate column position in current line
        const line_start = self.getLineStart(self.gap_start);
        const current_col = self.gap_start - line_start;

        // Find target position in next line
        var target_pos = next_line_start;
        var col_count: usize = 0;
        const next_line_end = self.getLineEnd(next_line_start);
        while (target_pos < next_line_end and col_count < current_col) {
            if (self.data[target_pos] == '\n') break;
            target_pos += 1;
            col_count += 1;
        }

        // Move gap to target position
        return self.moveGapTo(target_pos);
    }

    /// Move to start of buffer
    pub fn moveToStart(self: *GapBuffer) void {
        _ = self.moveGapTo(0);
    }

    /// Move to end of buffer
    pub fn moveToEnd(self: *GapBuffer) void {
        const end = self.getContentEnd();
        _ = self.moveGapTo(end);
    }

    /// Move to start of current line
    pub fn moveToLineStart(self: *GapBuffer) void {
        const line_start = self.getLineStart(self.gap_start);
        _ = self.moveGapTo(line_start);
    }

    /// Move to end of current line
    pub fn moveToLineEnd(self: *GapBuffer) void {
        const line_end = self.getLineEnd(self.gap_start);
        _ = self.moveGapTo(line_end);
    }

    /// Get line number at cursor (0-indexed)
    pub fn getLine(self: *const GapBuffer) usize {
        var line: usize = 0;
        var pos: usize = 0;
        const content_end = self.getContentEnd();

        while (pos < self.gap_start and pos < content_end) {
            if (self.data[pos] == '\n') {
                line += 1;
            }
            pos += 1;
        }

        return line;
    }

    /// Get total number of lines
    pub fn getTotalLines(self: *const GapBuffer) usize {
        var lines: usize = 1; // At least one line
        const content_end = self.getContentEnd();

        for (0..content_end) |i| {
            if (self.data[i] == '\n') {
                lines += 1;
            }
        }

        return lines;
    }

    /// Get start position of line containing pos
    fn getLineStart(self: *const GapBuffer, pos: usize) usize {
        var start = pos;

        // Adjust for gap
        if (start > self.gap_start) {
            start += (self.gap_end - self.gap_start);
        }

        // Find line start
        while (start > 0 and self.data[start - 1] != '\n') {
            start -= 1;
        }

        // Adjust back for gap
        if (start > self.gap_start) {
            start -= (self.gap_end - self.gap_start);
        }

        return start;
    }

    /// Get end position of line containing pos (position of newline or end of buffer)
    fn getLineEnd(self: *const GapBuffer, pos: usize) usize {
        var end = pos;
        const content_end = self.getContentEnd();

        // Adjust for gap
        if (end > self.gap_start) {
            end += (self.gap_end - self.gap_start);
        }

        // Find line end
        while (end < content_end and self.data[end] != '\n') {
            end += 1;
        }

        // Adjust back for gap
        if (end > self.gap_start) {
            end -= (self.gap_end - self.gap_start);
        }

        return end;
    }

    /// Get the end of actual content (excluding gap)
    fn getContentEnd(self: *const GapBuffer) usize {
        return self.data.len - (self.gap_end - self.gap_start);
    }

    /// Move gap to a specific position
    fn moveGapTo(self: *GapBuffer, target_pos: usize) bool {
        const content_end = self.getContentEnd();
        if (target_pos > content_end) return false;

        const current_pos = self.gap_start;

        if (target_pos == current_pos) return true;

        if (target_pos < current_pos) {
            // Move gap left: move bytes from before gap to after gap
            const bytes_to_move = current_pos - target_pos;
            var i: usize = 0;
            while (i < bytes_to_move) {
                self.gap_end -= 1;
                self.gap_start -= 1;
                self.data[self.gap_end] = self.data[self.gap_start];
                i += 1;
            }
        } else {
            // Move gap right: move bytes from after gap to before gap
            const bytes_to_move = target_pos - current_pos;
            var i: usize = 0;
            while (i < bytes_to_move) {
                self.data[self.gap_start] = self.data[self.gap_end];
                self.gap_start += 1;
                self.gap_end += 1;
                i += 1;
            }
        }

        return true;
    }

    /// Expand gap to accommodate at least min_size bytes
    fn expandGap(self: *GapBuffer, min_size: usize) !void {
        const current_gap_size = self.gap_end - self.gap_start;
        const needed_size = min_size + MIN_GAP_SIZE;
        const new_capacity = self.data.len + (needed_size - current_gap_size);

        const old_data = self.data;
        const new_data = try self.allocator.alloc(u8, new_capacity);

        // Copy data before gap
        @memcpy(new_data[0..self.gap_start], old_data[0..self.gap_start]);

        // Copy data after gap
        const after_gap_start = self.gap_end;
        const after_gap_len = old_data.len - after_gap_start;
        const new_gap_end = self.gap_start + needed_size;
        @memcpy(new_data[new_gap_end..new_gap_end + after_gap_len], old_data[after_gap_start..]);

        // Update buffer state only after successful allocation and copy
        self.data = new_data;
        self.gap_end = new_gap_end;
        
        // Free old data only after everything succeeded
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

    /// Delete current line (dd command)
    pub fn deleteLine(self: *GapBuffer) bool {
        const line_start = self.getLineStart(self.gap_start);
        const line_end = self.getLineEnd(self.gap_start);

        if (line_start == line_end and line_start >= self.getContentEnd()) {
            return false; // Empty buffer or last empty line
        }

        // Move gap to line start
        _ = self.moveGapTo(line_start);

        // Delete until line end (including newline if present)
        var deleted: usize = 0;
        const target_end = if (line_end < self.getContentEnd() and self.data[line_end] == '\n') line_end + 1 else line_end;

        while (self.gap_end < self.data.len and deleted < (target_end - line_start)) {
            self.gap_end += 1;
            deleted += 1;
        }

        return true;
    }
};
