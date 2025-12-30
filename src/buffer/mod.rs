//! Gap buffer implementation for efficient text editing
//!
//! REWRITTEN: Now uses Piece Table via LineIndex

use crate::error::RiftError;
use std::fmt::{self, Display};

pub mod line_index;
pub mod rope;
use line_index::LineIndex;

/// Text buffer using a Piece Table for efficient insertion and deletion.
pub struct TextBuffer {
    /// Line index which also holds the PieceTable
    pub line_index: LineIndex,
    /// Cursor position (byte offset)
    cursor: usize,
    /// Monotonic revision counter for change detection
    pub revision: u64,
}

impl TextBuffer {
    /// Create a new buffer
    pub fn new(_initial_capacity: usize) -> Result<Self, RiftError> {
        // Capacity is managed by the underlying PieceTable/Vec
        Ok(TextBuffer {
            line_index: LineIndex::new(),
            cursor: 0,
            revision: 0,
        })
    }

    /// Get the current cursor position
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Get the total length of text
    #[must_use]
    pub fn len(&self) -> usize {
        self.line_index.len()
    }

    /// Check if buffer is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.line_index.is_empty()
    }

    /// Move cursor left by one UTF-8 codepoint
    pub fn move_left(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            // Skip continuation bytes
            while self.cursor > 0 {
                let byte = self.line_index.byte_at(self.cursor);
                if (byte & 0b11000000) != 0b10000000 {
                    break;
                }
                self.cursor -= 1;
            }
            true
        } else {
            false
        }
    }

    /// Move cursor right by one UTF-8 codepoint
    pub fn move_right(&mut self) -> bool {
        let len = self.len();
        if self.cursor < len {
            self.cursor += 1;
            // Skip continuation bytes
            while self.cursor < len {
                let byte = self.line_index.byte_at(self.cursor);
                if (byte & 0b11000000) != 0b10000000 {
                    break;
                }
                self.cursor += 1;
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

    /// Insert bytes at the cursor position
    pub fn insert_bytes(&mut self, bytes: &[u8]) -> Result<(), RiftError> {
        self.line_index.insert(self.cursor, bytes);
        self.cursor += bytes.len();
        self.revision += 1;
        Ok(())
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) -> Result<(), RiftError> {
        self.insert_bytes(s.as_bytes())
    }

    /// Delete the UTF-8 codepoint before the cursor
    pub fn delete_backward(&mut self) -> bool {
        if self.cursor > 0 {
            let end = self.cursor;
            self.move_left(); // Moves cursor to start of char
            let start = self.cursor;
            let len = end - start;

            self.line_index.delete(start, len);
            self.revision += 1;
            true
        } else {
            false
        }
    }

    /// Delete the UTF-8 codepoint at the cursor position
    pub fn delete_forward(&mut self) -> bool {
        let len = self.len();
        if self.cursor < len {
            let start = self.cursor;
            let mut end = self.cursor + 1;

            while end < len {
                let byte = self.line_index.byte_at(end);
                if (byte & 0b11000000) != 0b10000000 {
                    break;
                }
                end += 1;
            }

            self.line_index.delete(start, end - start);
            self.revision += 1;
            true
        } else {
            false
        }
    }

    /// Get the text before the cursor
    #[must_use]
    pub fn get_before_gap(&self) -> Vec<u8> {
        self.line_index.bytes_range(0..self.cursor)
    }

    /// Get the text after the cursor
    #[must_use]
    pub fn get_after_gap(&self) -> Vec<u8> {
        self.line_index.bytes_range(self.cursor..self.len())
    }

    /// Get the line number at the cursor position
    #[must_use]
    pub fn get_line(&self) -> usize {
        self.line_index.get_line_at(self.cursor)
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

        self.line_index.bytes_range(start..end)
    }

    /// Move cursor up one line
    pub fn move_up(&mut self) -> bool {
        let current_line = self.get_line();
        if current_line == 0 {
            return false;
        }

        let prev_line = current_line - 1;
        let current_line_start = self.line_index.get_start(current_line).unwrap_or(0);
        let col = self.cursor - current_line_start;

        let prev_line_start = self.line_index.get_start(prev_line).unwrap_or(0);
        let prev_line_end = self.line_index.get_end(prev_line, self.len()).unwrap_or(0);

        // Target is min(start + col, end)
        // Note: get_end excludes newline. If we want to allow cursor at end of line, that's fine.
        let target = std::cmp::min(prev_line_start + col, prev_line_end);

        self.cursor = target;
        true
    }

    /// Move cursor down one line
    pub fn move_down(&mut self) -> bool {
        let current_line = self.get_line();
        let total_lines = self.get_total_lines();
        if current_line + 1 >= total_lines {
            return false;
        }

        let next_line = current_line + 1;
        let current_line_start = self.line_index.get_start(current_line).unwrap_or(0);
        let col = self.cursor - current_line_start;

        let next_line_start = self.line_index.get_start(next_line).unwrap_or(0);
        let next_line_end = self
            .line_index
            .get_end(next_line, self.len())
            .unwrap_or(self.len());

        let target = std::cmp::min(next_line_start + col, next_line_end);

        self.cursor = target;
        true
    }

    /// Move to start of buffer
    pub fn move_to_start(&mut self) {
        self.cursor = 0;
    }

    /// Move to end of buffer
    pub fn move_to_end(&mut self) {
        self.cursor = self.len();
    }

    /// Move to start of current line
    pub fn move_to_line_start(&mut self) {
        let line = self.get_line();
        if let Some(start) = self.line_index.get_start(line) {
            self.cursor = start;
        }
    }

    /// Move to end of current line
    pub fn move_to_line_end(&mut self) {
        let line = self.get_line();
        if let Some(end) = self.line_index.get_end(line, self.len()) {
            self.cursor = end;
        }
    }
}

impl Display for TextBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // This might be slow for large buffers, but Display is usually for debugging
        write!(f, "{}", self.line_index.table)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
