//! Gap buffer implementation for efficient text editing

use crate::error::{ErrorType, RiftError};
/// ## buffer/ Invariants
///
/// - The buffer owns both text storage and cursor position.
/// - The cursor is always located at the gap.
/// - `gap_start <= gap_end` at all times.
/// - All text before the gap is logically before the cursor.
/// - All text after the gap is logically after the cursor.
/// - Buffer contents are treated consistently as either UTF-8 or raw bytes.
/// - Movement operations never mutate text.
/// - Insert and delete operations never leave the buffer in an invalid state.
/// - Buffer methods either succeed fully or perform no mutation.
/// - The buffer never emits or interprets commands.
use std::alloc::{alloc, dealloc, Layout};
use std::fmt::{self, Display};

pub mod line_index;
use line_index::LineIndex;

/// Gap buffer for efficient insertion and deletion.
///
/// ## UTF-8 Invariant
/// The cursor (`gap_start`) is always positioned at the start of a UTF-8 codepoint.
/// All movement and deletion operations must maintain this invariant.
pub struct GapBuffer {
    /// Buffer containing text before gap, gap, and text after gap
    /// Layout: [`before_gap`][gap][`after_gap`]
    buffer: *mut u8,
    /// Capacity of the buffer
    capacity: usize,
    /// Start of gap (end of `before_gap`)
    gap_start: usize,
    /// End of gap (start of `after_gap`)
    gap_end: usize,
    /// Line index for efficient line lookup
    pub line_index: LineIndex,
    /// Monotonic revision counter for change detection
    pub revision: u64,
}

impl GapBuffer {
    /// Create a new gap buffer with initial capacity
    pub fn new(initial_capacity: usize) -> Result<Self, RiftError> {
        if initial_capacity == 0 {
            return Err(RiftError::new(
                ErrorType::Internal,
                "INVALID_CAPACITY",
                "Capacity must be > 0".to_string(),
            ));
        }

        let layout = Layout::from_size_align(initial_capacity, 1).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "INVALID_LAYOUT",
                format!("Invalid layout: {e}"),
            )
        })?;

        let buffer = unsafe { alloc(layout) };
        if buffer.is_null() {
            return Err(RiftError::new(
                ErrorType::Internal,
                "ALLOC_FAILED",
                "Failed to allocate buffer".to_string(),
            ));
        }

        Ok(GapBuffer {
            buffer,
            capacity: initial_capacity,
            gap_start: 0,
            gap_end: initial_capacity,
            line_index: LineIndex::new(),
            revision: 0,
        })
    }

    /// Get the current cursor position (same as `gap_start`)
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.gap_start
    }

    /// Get the total length of text (excluding gap)
    #[must_use]
    pub fn len(&self) -> usize {
        self.gap_start + (self.capacity - self.gap_end)
    }

    /// Check if buffer is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Move cursor left (move gap right) by one UTF-8 codepoint
    pub fn move_left(&mut self) -> bool {
        if self.gap_start > 0 {
            unsafe {
                // Move one byte
                let first_byte = *self.buffer.add(self.gap_start - 1);
                *self.buffer.add(self.gap_end - 1) = first_byte;
                self.gap_start -= 1;
                self.gap_end -= 1;

                // Only continue if we moved a continuation byte (meaning we are in the middle of a char)
                if (first_byte & 0b11000000) == 0b10000000 {
                    while self.gap_start > 0 {
                        let byte = *self.buffer.add(self.gap_start - 1);
                        *self.buffer.add(self.gap_end - 1) = byte;
                        self.gap_start -= 1;
                        self.gap_end -= 1;

                        // Stop if we just moved the header byte
                        if (byte & 0b11000000) != 0b10000000 {
                            break;
                        }
                    }
                }
            }
            true
        } else {
            false
        }
    }

    /// Move cursor right (move gap left) by one UTF-8 codepoint
    pub fn move_right(&mut self) -> bool {
        if self.gap_end < self.capacity {
            // Move right one byte
            unsafe {
                let byte = *self.buffer.add(self.gap_end);
                *self.buffer.add(self.gap_start) = byte;
            }
            self.gap_start += 1;
            self.gap_end += 1;

            // Skip all continuation bytes
            while self.gap_end < self.capacity {
                let byte = unsafe { *self.buffer.add(self.gap_end) };
                if (byte & 0b11000000) != 0b10000000 {
                    break;
                }
                unsafe {
                    let b = *self.buffer.add(self.gap_end);
                    *self.buffer.add(self.gap_start) = b;
                }
                self.gap_start += 1;
                self.gap_end += 1;
            }
            true
        } else {
            false
        }
    }

    /// Insert a byte at the cursor position
    pub fn insert(&mut self, byte: u8) -> Result<(), RiftError> {
        self.insert_bytes(&[byte])
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, ch: char) -> Result<(), RiftError> {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        self.insert_bytes(s.as_bytes())
    }

    /// Insert bytes at the cursor position (batch insertion)
    /// More efficient than inserting byte-by-byte
    pub fn insert_bytes(&mut self, bytes: &[u8]) -> Result<(), RiftError> {
        if bytes.is_empty() {
            return Ok(());
        }

        let needed = bytes.len();
        let mut available = self.gap_end - self.gap_start;

        // Grow buffer if needed to fit all bytes
        while available < needed {
            self.grow()?;
            // Recalculate available after growth
            available = self.gap_end - self.gap_start;
        }

        // Copy all bytes at once
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.buffer.add(self.gap_start), needed);
        }
        self.line_index.insert(self.gap_start, bytes);
        self.gap_start += needed;
        self.revision += 1;

        Ok(())
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) -> Result<(), RiftError> {
        self.insert_bytes(s.as_bytes())
    }

    /// Delete the UTF-8 codepoint before the cursor (backspace)
    pub fn delete_backward(&mut self) -> bool {
        if self.gap_start > 0 {
            let mut bytes_to_delete = 1;
            // Count continuation bytes
            while self.gap_start > bytes_to_delete {
                let byte = unsafe { *self.buffer.add(self.gap_start - bytes_to_delete - 1) };
                if (byte & 0b11000000) != 0b10000000 {
                    break;
                }
                bytes_to_delete += 1;
            }
            // Also need to check the byte at (gap_start - bytes_to_delete) - it should be the start byte

            self.line_index
                .delete(self.gap_start - bytes_to_delete, bytes_to_delete);
            self.gap_start -= bytes_to_delete;
            self.revision += 1;
            true
        } else {
            false
        }
    }

    /// Delete the UTF-8 codepoint at the cursor position (delete)
    pub fn delete_forward(&mut self) -> bool {
        if self.gap_end < self.capacity {
            let mut bytes_to_delete = 1;
            // Skip continuation bytes
            while self.gap_end + bytes_to_delete < self.capacity {
                let byte = unsafe { *self.buffer.add(self.gap_end + bytes_to_delete) };
                if (byte & 0b11000000) != 0b10000000 {
                    break;
                }
                bytes_to_delete += 1;
            }

            self.line_index.delete(self.gap_start, bytes_to_delete);
            self.gap_end += bytes_to_delete;
            self.revision += 1;
            true
        } else {
            false
        }
    }

    /// Get the text before the gap as a string
    #[must_use]
    pub fn get_before_gap(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.buffer, self.gap_start) }
    }

    /// Get the text after the gap as a string
    #[must_use]
    pub fn get_after_gap(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.buffer.add(self.gap_end), self.capacity - self.gap_end)
        }
    }

    /// Get the line number at the cursor position
    #[must_use]
    pub fn get_line(&self) -> usize {
        self.line_index.get_line_at(self.gap_start)
    }

    /// Get the total number of lines
    #[must_use]
    pub fn get_total_lines(&self) -> usize {
        self.line_index.line_count()
    }

    /// Get bytes for a specific line (excluding trailing newline)
    #[must_use]
    pub fn get_line_bytes(&self, line_idx: usize) -> Vec<u8> {
        let start = match self.line_index.get_start(line_idx) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let end = match self.line_index.get_end(line_idx, self.len()) {
            Some(e) => e,
            None => return Vec::new(),
        };

        if end <= start {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(end - start);

        // 1. Portion before gap
        if start < self.gap_start {
            let chunk_end = end.min(self.gap_start);
            let before = self.get_before_gap();
            result.extend_from_slice(&before[start..chunk_end]);
        }

        // 2. Portion after gap
        if end > self.gap_start {
            let chunk_start = start.max(self.gap_start);
            let after = self.get_after_gap();
            // Logical 'chunk_start' is 'chunk_start - gap_start' in the 'after' slice
            let slice_start = chunk_start.saturating_sub(self.gap_start);
            let slice_end = end.saturating_sub(self.gap_start);
            if slice_start < after.len() {
                let actual_end = slice_end.min(after.len());
                result.extend_from_slice(&after[slice_start..actual_end]);
            }
        }

        result
    }

    /// Move cursor up one line
    pub fn move_up(&mut self) -> bool {
        let line_start = self.get_line_start(self.gap_start);
        if line_start == 0 {
            return false; // Already at first line
        }

        // Find start of previous line
        let mut prev_line_start = line_start;
        if prev_line_start > 0 {
            prev_line_start -= 1;
            let before = self.get_before_gap();
            while prev_line_start > 0 && before[prev_line_start - 1] != b'\n' {
                prev_line_start -= 1;
            }
        }

        // Calculate column position in current line
        let current_col = self.gap_start - line_start;

        // Find target position in previous line
        let mut target_pos = prev_line_start;
        let mut col_count = 0;
        let before = self.get_before_gap();
        while target_pos < line_start && col_count < current_col {
            if target_pos < before.len() && before[target_pos] == b'\n' {
                break;
            }
            target_pos += 1;
            col_count += 1;
        }

        // Move gap to target position
        self.move_gap_to(target_pos)
    }

    /// Move cursor down one line
    pub fn move_down(&mut self) -> bool {
        let line_end = self.get_line_end(self.gap_start);
        let content_end = self.len();
        if line_end >= content_end {
            return false; // Already at last line
        }

        // Find start of next line
        let next_line_start = line_end + 1;
        if next_line_start >= content_end {
            return false;
        }

        // Calculate column position in current line
        let line_start = self.get_line_start(self.gap_start);
        let current_col = self.gap_start - line_start;

        // Find target position in next line
        let mut target_pos = next_line_start;
        let mut col_count = 0;
        let next_line_end = self.get_line_end(next_line_start);

        // Get byte at position (handling gap)
        while target_pos < next_line_end && col_count < current_col {
            let byte = self.get_byte_at(target_pos);
            if byte == Some(b'\n') {
                break;
            }
            if byte.is_none() {
                break;
            }
            target_pos += 1;
            col_count += 1;
        }

        // Move gap to target position
        self.move_gap_to(target_pos)
    }

    /// Get byte at a specific position (handles gap)
    /// pos is a logical position (0 to len()-1), not a physical buffer position
    fn get_byte_at(&self, pos: usize) -> Option<u8> {
        if pos >= self.len() {
            // Position out of bounds
            None
        } else if pos < self.gap_start {
            // Before gap - physical position equals logical position
            unsafe { Some(*self.buffer.add(pos)) }
        } else {
            // After gap - convert logical to physical position
            // Physical = logical + gap_size
            let gap_size = self.gap_end - self.gap_start;
            let physical_pos = pos + gap_size;
            unsafe { Some(*self.buffer.add(physical_pos)) }
        }
    }

    /// Move to start of buffer
    pub fn move_to_start(&mut self) {
        let _ = self.move_gap_to(0);
    }

    /// Move to end of buffer
    pub fn move_to_end(&mut self) {
        let end = self.len();
        let _ = self.move_gap_to(end);
    }

    /// Move to start of current line
    pub fn move_to_line_start(&mut self) {
        let line_start = self.get_line_start(self.gap_start);
        let _ = self.move_gap_to(line_start);
    }

    /// Move to end of current line
    pub fn move_to_line_end(&mut self) {
        let line_end = self.get_line_end(self.gap_start);
        let _ = self.move_gap_to(line_end);
    }

    /// Get the start of the line containing position
    fn get_line_start(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }

        let before = self.get_before_gap();
        let mut line_start = pos.min(before.len());

        // Search backwards for newline
        while line_start > 0 && before[line_start - 1] != b'\n' {
            line_start -= 1;
        }

        line_start
    }

    /// Get the end of the line containing position (position of newline or end of buffer)
    fn get_line_end(&self, pos: usize) -> usize {
        let before = self.get_before_gap();
        let after = self.get_after_gap();
        let total_len = self.len();

        // Start searching from position
        let mut search_pos = pos;

        // Check before gap
        if search_pos < before.len() {
            for (i, bf) in before.iter().enumerate().skip(search_pos) {
                if *bf == b'\n' {
                    return i;
                }
            }
            search_pos = before.len();
        }

        // Check after gap
        let _after_start = self.gap_end;
        let after_offset = search_pos - before.len();
        if after_offset < after.len() {
            for (i, af) in after.iter().enumerate().skip(after_offset) {
                if *af == b'\n' {
                    return before.len() + i;
                }
            }
        }

        // No newline found, return end of buffer
        total_len
    }

    /// Move gap to a specific position
    fn move_gap_to(&mut self, target_pos: usize) -> bool {
        let current_pos = self.gap_start;

        if target_pos == current_pos {
            return true; // Already at target
        }

        if target_pos > self.len() {
            return false; // Invalid position
        }

        // Move gap to target by shifting bytes
        if target_pos < current_pos {
            // Move gap left (move bytes from before_gap to after_gap)
            let bytes_to_move = current_pos - target_pos;
            for _ in 0..bytes_to_move {
                if !self.move_left() {
                    return false;
                }
            }
        } else {
            // Move gap right (move bytes from after_gap to before_gap)
            let bytes_to_move = target_pos - current_pos;
            for _ in 0..bytes_to_move {
                if !self.move_right() {
                    return false;
                }
            }
        }

        true
    }

    /// Grow the buffer when gap is exhausted
    fn grow(&mut self) -> Result<(), RiftError> {
        let new_capacity = self.capacity * 2;
        let new_layout = Layout::from_size_align(new_capacity, 1).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "INVALID_LAYOUT",
                format!("Invalid layout: {e}"),
            )
        })?;

        let new_buffer = unsafe { alloc(new_layout) };
        if new_buffer.is_null() {
            return Err(RiftError::new(
                ErrorType::Internal,
                "ALLOC_FAILED",
                "Failed to allocate new buffer".to_string(),
            ));
        }

        // Copy before_gap
        unsafe {
            std::ptr::copy_nonoverlapping(self.buffer, new_buffer, self.gap_start);
        }

        // Copy after_gap to the end
        let after_len = self.capacity - self.gap_end;
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.buffer.add(self.gap_end),
                new_buffer.add(new_capacity - after_len),
                after_len,
            );
        }

        // Deallocate old buffer
        let old_layout = Layout::from_size_align(self.capacity, 1).unwrap();
        unsafe {
            dealloc(self.buffer, old_layout);
        }

        self.gap_end = new_capacity - after_len;
        self.capacity = new_capacity;
        self.buffer = new_buffer;

        Ok(())
    }
}

impl Drop for GapBuffer {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.capacity, 1).unwrap();
        unsafe {
            dealloc(self.buffer, layout);
        }
    }
}
impl Display for GapBuffer {
    /// Get the entire text as a string (reconstructs by moving gap to end)
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let before = self.get_before_gap();
        let after = self.get_after_gap();
        let mut result = Vec::with_capacity(before.len() + after.len());
        result.extend_from_slice(before);
        result.extend_from_slice(after);
        write!(f, "{}", String::from_utf8_lossy(&result))
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
