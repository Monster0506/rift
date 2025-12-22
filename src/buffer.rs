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
    pub fn delete_backward(&mut self) -> bool {
        if self.gap_start > 0 {
            // Move gap left by one
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
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let buf = GapBuffer::new(10).unwrap();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn test_insert() {
        let mut buf = GapBuffer::new(10).unwrap();
        buf.insert(b'a').unwrap();
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.cursor(), 1);
    }

    #[test]
    fn test_move_and_insert() {
        let mut buf = GapBuffer::new(10).unwrap();
        buf.insert_str("hello").unwrap();
        // Move to start
        for _ in 0..5 {
            buf.move_left();
        }
        buf.insert(b'X').unwrap();
        assert_eq!(buf.to_string(), "Xhello");
    }

    #[test]
    fn test_delete() {
        let mut buf = GapBuffer::new(10).unwrap();
        buf.insert_str("hello").unwrap();
        buf.move_left();
        buf.delete_backward().unwrap();
        assert_eq!(buf.to_string(), "hell");
    }
}

