//! Gap buffer implementation for efficient text editing

use crate::buffer::api::BufferView;
use crate::error::RiftError;
use std::fmt::{self, Display};

pub mod api;
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

    pub fn set_cursor(&mut self, pos: usize) -> Result<(), RiftError> {
        let len = self.len();
        if pos > len {
            return Err(RiftError::new(
                crate::error::ErrorType::Internal,
                crate::constants::error_types::INVALID_CURSOR,
                format!("Cursor position {} out of bounds (len: {})", pos, len),
            ));
        }

        let current = self.cursor;
        if pos < current {
            for _ in 0..(current - pos) {
                self.move_left();
            }
        } else if pos > current {
            for _ in 0..(pos - current) {
                self.move_right();
            }
        }
        Ok(())
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

    /// Get a chunk of text starting at the given byte offset.
    /// Used for Tree-sitter integration.
    pub fn get_chunk_at_byte(&self, pos: usize) -> &[u8] {
        self.line_index.get_chunk_at_byte(pos)
    }

    pub fn byte_at(&self, pos: usize) -> u8 {
        self.line_index.byte_at(pos)
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

    /// Move to the start of the next word
    pub fn move_word_right(&mut self) -> bool {
        let len = self.len();
        if self.cursor >= len {
            return false;
        }

        let get_class = |c: char| -> u8 {
            if c.is_whitespace() {
                0
            } else if c.is_alphanumeric() || c == '_' {
                1
            } else {
                2
            }
        };

        let start_pos = self.cursor;

        // Get current char
        let curr_char = self.char_at(self.cursor);
        if curr_char.is_none() {
            return false;
        }

        let start_class = get_class(curr_char.unwrap());

        if start_class == 0 {
            // If on whitespace, skip whitespace
            while self.cursor < len {
                if let Some(c) = self.char_at(self.cursor) {
                    if !c.is_whitespace() {
                        break;
                    }
                }
                self.move_right();
            }
        } else {
            // Skip current word/punct
            while self.cursor < len {
                if let Some(c) = self.char_at(self.cursor) {
                    if get_class(c) != start_class {
                        break;
                    }
                }
                self.move_right();
            }
            // Skip whitespace
            while self.cursor < len {
                if let Some(c) = self.char_at(self.cursor) {
                    if !c.is_whitespace() {
                        break;
                    }
                }
                self.move_right();
            }
        }

        self.cursor != start_pos
    }

    /// Move to the start of the previous word
    pub fn move_word_left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        let get_class = |c: char| -> u8 {
            if c.is_whitespace() {
                0
            } else if c.is_alphanumeric() || c == '_' {
                1
            } else {
                2
            }
        };

        let start_pos = self.cursor;

        // Move left once to start checking
        self.move_left();

        // Skip whitespace going backwards
        while self.cursor > 0 {
            if let Some(c) = self.char_at(self.cursor) {
                if !c.is_whitespace() {
                    break;
                }
            }
            self.move_left();
        }

        // Now we are on the last char of the previous word (or start of file)
        // We need to find the start of this word.

        if let Some(c) = self.char_at(self.cursor) {
            let target_class = get_class(c);
            if target_class == 0 {
                // Still whitespace? means we hit start of file with whitespace
                return true;
            }

            // Go back until class changes
            while self.cursor > 0 {
                // Look at previous char without moving yet
                let prev_pos = self.prev_char_pos(self.cursor);
                if let Some(pc) = self.char_at(prev_pos) {
                    if get_class(pc) != target_class {
                        break;
                    }
                }
                self.move_left();
            }
        }

        self.cursor != start_pos
    }

    /// Move to next paragraph
    pub fn move_paragraph_forward(&mut self) -> bool {
        let start_cursor = self.cursor;
        let current_line = self.get_line();
        let total_lines = self.get_total_lines();

        if current_line >= total_lines - 1 {
            self.move_to_end();
            return self.cursor != start_cursor;
        }

        let mut line = current_line + 1;
        while line < total_lines {
            if self.is_line_empty(line) {
                // Found empty line
                if let Some(start) = self.line_index.get_start(line) {
                    self.cursor = start;
                }
                return self.cursor != start_cursor;
            }
            line += 1;
        }

        self.move_to_end();
        self.cursor != start_cursor
    }

    /// Move to previous paragraph
    pub fn move_paragraph_backward(&mut self) -> bool {
        let start_cursor = self.cursor;
        let current_line = self.get_line();

        if current_line == 0 {
            self.move_to_start();
            return self.cursor != start_cursor;
        }

        let mut line = current_line - 1;
        while line > 0 {
            if self.is_line_empty(line) {
                if let Some(start) = self.line_index.get_start(line) {
                    self.cursor = start;
                }
                return self.cursor != start_cursor;
            }
            line -= 1;
        }

        self.move_to_start();
        self.cursor != start_cursor
    }

    /// Move to next sentence
    pub fn move_sentence_forward(&mut self) -> bool {
        let len = self.len();
        if self.cursor >= len {
            return false;
        }

        let start_pos = self.cursor;

        // Scan forward
        while self.cursor < len {
            let c = self.char_at(self.cursor);

            // If we hit a newline without finding a sentence end, stop there
            if let Some('\n') = c {
                if self.cursor != start_pos {
                    return true;
                }
            }

            self.move_right();

            if let Some(ch) = c {
                if ".!?".contains(ch) {
                    // Check if followed by whitespace (or EOF)
                    if self.cursor == len {
                        return true;
                    }
                    if let Some(next_ch) = self.char_at(self.cursor) {
                        if next_ch.is_whitespace() {
                            // Found end of sentence. Now skip whitespace to find start of next.
                            while self.cursor < len {
                                if let Some(nc) = self.char_at(self.cursor) {
                                    if !nc.is_whitespace() {
                                        return true;
                                    }
                                }
                                self.move_right();
                            }
                            return true; // EOF is valid start
                        }
                    }
                }
            }
        }

        self.cursor != start_pos
    }

    /// Move to previous sentence
    pub fn move_sentence_backward(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        let start_pos = self.cursor;

        // 1. Move left
        self.move_left();

        // 2. Skip whitespace backwards
        while self.cursor > 0 {
            if let Some(c) = self.char_at(self.cursor) {
                if !c.is_whitespace() {
                    break;
                }
            }
            self.move_left();
        }

        // 3. Skip terminators backwards (in case we were at start of sentence)
        while self.cursor > 0 {
            if let Some(c) = self.char_at(self.cursor) {
                if !".!?".contains(c) {
                    break;
                }
            }
            self.move_left();
        }

        // 4. Scan backwards for terminator
        while self.cursor > 0 {
            if let Some(c) = self.char_at(self.cursor) {
                if ".!?".contains(c) {
                    // Found terminator of previous sentence.
                    self.move_right();
                    break;
                }
            }
            self.move_left();
        }

        // 5. Skip whitespace forward
        while self.cursor < self.len() {
            if let Some(c) = self.char_at(self.cursor) {
                if !c.is_whitespace() {
                    break;
                }
            }
            self.move_right();
        }

        self.cursor != start_pos
    }

    fn is_line_empty(&self, line_idx: usize) -> bool {
        let start = match self.line_index.get_start(line_idx) {
            Some(s) => s,
            None => return true,
        };
        let end = match self.line_index.get_end(line_idx, self.len()) {
            Some(e) => e,
            None => return true,
        };

        for i in start..end {
            let b = self.line_index.byte_at(i);
            if !b.is_ascii_whitespace() {
                return false;
            }
        }
        true
    }

    fn char_at(&self, pos: usize) -> Option<char> {
        if pos >= self.len() {
            return None;
        }

        let b = self.line_index.byte_at(pos);
        if b < 128 {
            return Some(b as char);
        }

        let mut bytes = [0u8; 4];
        bytes[0] = b;
        let mut len = 1;

        let mut p = pos + 1;
        while p < self.len() && len < 4 {
            let next_b = self.line_index.byte_at(p);
            if (next_b & 0b11000000) != 0b10000000 {
                break;
            }
            bytes[len] = next_b;
            len += 1;
            p += 1;
        }

        std::str::from_utf8(&bytes[..len])
            .ok()
            .and_then(|s| s.chars().next())
    }

    fn prev_char_pos(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut p = pos - 1;
        while p > 0 {
            let byte = self.line_index.byte_at(p);
            if (byte & 0b11000000) != 0b10000000 {
                break;
            }
            p -= 1;
        }
        p
    }
}

impl Display for TextBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // This might be slow for large buffers, but Display is usually for debugging
        write!(f, "{}", self.line_index.table)
    }
}

impl BufferView for TextBuffer {
    fn len(&self) -> usize {
        self.len()
    }

    fn line_count(&self) -> usize {
        self.get_total_lines()
    }

    fn line_start(&self, line: usize) -> usize {
        self.line_index.get_line_start(line)
    }

    fn line_bytes(&self, line: usize) -> impl Iterator<Item = &[u8]> + '_ {
        let start = self.line_index.get_line_start(line);
        let end = self.line_index.get_end(line, self.len()).unwrap_or(start);
        self.line_index.chunks_in_range(start..end)
    }

    fn slice(&self, start: usize, end: usize) -> impl Iterator<Item = &[u8]> + '_ {
        self.line_index.chunks_in_range(start..end)
    }

    fn revision(&self) -> u64 {
        self.revision
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
