//! Gap buffer implementation for efficient text editing

use std::alloc::{alloc, dealloc, Layout};

/// Gap buffer for efficient insertion and deletion
pub struct GapBuffer {
    /// Buffer containing text before gap, gap, and text after gap
    /// Layout: [before_gap][gap][after_gap]
    buffer: *mut u8,
    /// Capacity of the buffer
    capacity: usize,
    /// Start of gap (end of before_gap)
    gap_start: usize,
    /// End of gap (start of after_gap)
    gap_end: usize,
}

impl GapBuffer {
    /// Create a new gap buffer with initial capacity
    pub fn new(initial_capacity: usize) -> Result<Self, String> {
        if initial_capacity == 0 {
            return Err("Capacity must be > 0".to_string());
        }

        let layout = Layout::from_size_align(initial_capacity, 1)
            .map_err(|e| format!("Invalid layout: {}", e))?;
        
        let buffer = unsafe { alloc(layout) };
        if buffer.is_null() {
            return Err("Failed to allocate buffer".to_string());
        }

        Ok(GapBuffer {
            buffer,
            capacity: initial_capacity,
            gap_start: 0,
            gap_end: initial_capacity,
        })
    }

    /// Get the current cursor position (same as gap_start)
    pub fn cursor(&self) -> usize {
        self.gap_start
    }

    /// Get the total length of text (excluding gap)
    pub fn len(&self) -> usize {
        self.gap_start + (self.capacity - self.gap_end)
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Move cursor left (move gap right)
    pub fn move_left(&mut self) -> bool {
        if self.gap_start > 0 {
            unsafe {
                let byte = *self.buffer.add(self.gap_start - 1);
                *self.buffer.add(self.gap_end - 1) = byte;
            }
            self.gap_start -= 1;
            self.gap_end -= 1;
            true
        } else {
            false
        }
    }

    /// Move cursor right (move gap left)
    pub fn move_right(&mut self) -> bool {
        if self.gap_end < self.capacity {
            unsafe {
                let byte = *self.buffer.add(self.gap_end);
                *self.buffer.add(self.gap_start) = byte;
            }
            self.gap_start += 1;
            self.gap_end += 1;
            true
        } else {
            false
        }
    }

    /// Insert a byte at the cursor position
    pub fn insert(&mut self, byte: u8) -> Result<(), String> {
        if self.gap_start >= self.gap_end {
            // Gap is exhausted, need to grow
            self.grow()?;
        }

        unsafe {
            *self.buffer.add(self.gap_start) = byte;
        }
        self.gap_start += 1;
        Ok(())
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) -> Result<(), String> {
        for byte in s.bytes() {
            self.insert(byte)?;
        }
        Ok(())
    }

    /// Delete the byte before the cursor (backspace)
    /// This moves the gap left, effectively deleting the byte
    pub fn delete_backward(&mut self) -> bool {
        if self.gap_start > 0 {
            // Just move gap left - the byte is effectively deleted
            self.gap_start -= 1;
            true
        } else {
            false
        }
    }

    /// Delete the byte at the cursor position (delete)
    pub fn delete_forward(&mut self) -> bool {
        if self.gap_end < self.capacity {
            // Just expand gap by one (effectively deleting the byte)
            self.gap_end += 1;
            true
        } else {
            false
        }
    }

    /// Get the text before the gap as a string
    pub fn get_before_gap(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.buffer, self.gap_start)
        }
    }

    /// Get the text after the gap as a string
    pub fn get_after_gap(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.buffer.add(self.gap_end), self.capacity - self.gap_end)
        }
    }

    /// Get the entire text as a string (reconstructs by moving gap to end)
    pub fn to_string(&self) -> String {
        let before = self.get_before_gap();
        let after = self.get_after_gap();
        let mut result = Vec::with_capacity(before.len() + after.len());
        result.extend_from_slice(before);
        result.extend_from_slice(after);
        String::from_utf8_lossy(&result).to_string()
    }

    /// Get the line number at the cursor position
    pub fn get_line(&self) -> usize {
        let before = self.get_before_gap();
        before.iter().filter(|&&b| b == b'\n').count()
    }

    /// Get the total number of lines
    pub fn get_total_lines(&self) -> usize {
        let before = self.get_before_gap();
        let after = self.get_after_gap();
        let newlines = before.iter().filter(|&&b| b == b'\n').count()
            + after.iter().filter(|&&b| b == b'\n').count();
        // If there's at least one newline, lines = newlines + 1
        // If no newlines, lines = 1 (single line)
        if newlines > 0 || !before.is_empty() || !after.is_empty() {
            newlines + 1
        } else {
            0
        }
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
    fn get_byte_at(&self, pos: usize) -> Option<u8> {
        if pos < self.gap_start {
            // Before gap
            unsafe {
                Some(*self.buffer.add(pos))
            }
        } else if pos >= self.gap_end {
            // After gap - need to adjust for gap size
            let adjusted_pos = pos - (self.gap_end - self.gap_start);
            if adjusted_pos < self.capacity {
                unsafe {
                    Some(*self.buffer.add(adjusted_pos))
                }
            } else {
                None
            }
        } else {
            // In gap - invalid position
            None
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
            for i in search_pos..before.len() {
                if before[i] == b'\n' {
                    return i;
                }
            }
            search_pos = before.len();
        }
        
        // Check after gap
        let _after_start = self.gap_end;
        let after_offset = search_pos - before.len();
        if after_offset < after.len() {
            for i in after_offset..after.len() {
                if after[i] == b'\n' {
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
    fn grow(&mut self) -> Result<(), String> {
        let new_capacity = self.capacity * 2;
        let new_layout = Layout::from_size_align(new_capacity, 1)
            .map_err(|e| format!("Invalid layout: {}", e))?;

        let new_buffer = unsafe { alloc(new_layout) };
        if new_buffer.is_null() {
            return Err("Failed to allocate new buffer".to_string());
        }

        // Copy before_gap
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.buffer,
                new_buffer,
                self.gap_start,
            );
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
