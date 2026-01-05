//! Text Buffer implementation backed by a Piece Table
//!
//! This module provides a `TextBuffer` that manages text using a piece table data structure.
//! It supports efficient insertion and deletion, and handles line indexing.

use crate::buffer::api::BufferView;
use crate::character::Character;
use crate::error::RiftError;
use std::fmt::{self, Display};
use std::ops::Range;

pub mod api;
pub mod line_index;
pub mod rope;
use line_index::LineIndex;

/// Text buffer using a Piece Table for efficient insertion and deletion.
#[derive(Clone)]
pub struct TextBuffer {
    /// Line index which also holds the PieceTable
    pub line_index: LineIndex,
    /// Cursor position (Character index)
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

        self.cursor = pos;
        Ok(())
    }

    /// Get the total length of text (in Characters)
    #[must_use]
    pub fn len(&self) -> usize {
        self.line_index.len()
    }

    /// Check if buffer is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.line_index.is_empty()
    }

    /// Move cursor left by one Character
    pub fn move_left(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    /// Move cursor right by one Character
    pub fn move_right(&mut self) -> bool {
        let len = self.len();
        if self.cursor < len {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// Insert a byte at the cursor position
    pub fn insert(&mut self, byte: u8) -> Result<(), RiftError> {
        let ch = Character::from(byte);
        self.insert_chars(&[ch])
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, ch: char) -> Result<(), RiftError> {
        let character = Character::from(ch);
        self.insert_chars(&[character])
    }

    /// Insert bytes at the cursor position
    pub fn insert_bytes(&mut self, bytes: &[u8]) -> Result<(), RiftError> {
        let chars: Vec<Character> = bytes.iter().map(|&b| Character::from(b)).collect();
        self.insert_chars(&chars)
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) -> Result<(), RiftError> {
        let chars: Vec<Character> = s.chars().map(Character::from).collect();
        self.insert_chars(&chars)
    }

    /// Internal insert helper
    fn insert_chars(&mut self, chars: &[Character]) -> Result<(), RiftError> {
        self.line_index.insert(self.cursor, chars);
        self.cursor += chars.len();
        self.revision += 1;
        Ok(())
    }

    /// Delete the Character before the cursor
    pub fn delete_backward(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.line_index.delete(self.cursor, 1);
            self.revision += 1;
            true
        } else {
            false
        }
    }

    /// Delete the Character at the cursor position
    pub fn delete_forward(&mut self) -> bool {
        if self.cursor < self.len() {
            self.line_index.delete(self.cursor, 1);
            self.revision += 1;
            true
        } else {
            false
        }
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

    /// Convert byte offset to character index
    pub fn byte_to_char(&self, byte_offset: usize) -> usize {
        self.line_index.byte_to_char(byte_offset)
    }

    /// Convert character index to byte offset
    pub fn char_to_byte(&self, char_index: usize) -> usize {
        self.line_index.char_to_byte(char_index)
    }

    /// Get bytes for a specific line (excluding trailing newline)
    /// Note: This reconstructs bytes from Characters.
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
    /// This is now tricky because strict Character abstraction.
    /// We will return bytes_range from current char index?
    /// Tree-sitter works on bytes. We might need to map byte offset -> char offset.
    /// This is complex. For now, we stub or use simple mapping if 1-to-1.
    /// But Character is NOT 1-to-1 with bytes necessarily if we inserted Unicode chars.
    /// If we want Tree-sitter, we need byte_len in PieceTable nodes (which we added).
    /// But we need API for byte_to_char_idx.
    pub fn get_chunk_at_byte(&self, _pos: usize) -> &[u8] {
        // TODO: Implement proper byte-to-char mapping and chunking for Tree-sitter.
        // For now return empty to avoid panic, or implement panic to find usage.
        &[]
    }

    pub fn char_at(&self, pos: usize) -> Option<Character> {
        if pos >= self.len() {
            None
        } else {
            Some(self.line_index.char_at(pos))
        }
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

    // Legacy/Helper methods relying on Character properties

    pub fn move_word_right(&mut self) -> bool {
        self.move_generic_word_right(false)
    }

    pub fn move_big_word_right(&mut self) -> bool {
        self.move_generic_word_right(true)
    }

    fn move_generic_word_right(&mut self, big_word: bool) -> bool {
        let len = self.len();
        if self.cursor >= len {
            return false;
        }
        let start_pos = self.cursor;

        let get_cat = |c: Character| -> u8 {
            if c.to_char_lossy().is_whitespace() {
                0 // Whitespace
            } else if big_word {
                1 // Non-whitespace (Big Word)
            } else if c.to_char_lossy().is_alphanumeric() || c.to_char_lossy() == '_' {
                2 // Alphanumeric
            } else {
                3 // Symbol
            }
        };

        if let Some(curr) = self.char_at(self.cursor) {
            let start_cat = get_cat(curr);

            // 1. Skip current word category
            while self.cursor < len {
                match self.char_at(self.cursor) {
                    Some(c) if get_cat(c) == start_cat => {
                        self.move_right();
                    }
                    _ => break,
                }
            }

            // 2. Skip whitespace if we weren't already on whitespace
            if start_cat != 0 {
                while self.cursor < len {
                    match self.char_at(self.cursor) {
                        Some(c) if get_cat(c) == 0 => {
                            self.move_right();
                        }
                        _ => break,
                    }
                }
            }
        }

        self.cursor != start_pos
    }

    pub fn move_word_left(&mut self) -> bool {
        self.move_generic_word_left(false)
    }

    pub fn move_big_word_left(&mut self) -> bool {
        self.move_generic_word_left(true)
    }

    fn move_generic_word_left(&mut self, big_word: bool) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let start_pos = self.cursor;

        let get_cat = |c: Character| -> u8 {
            if c.to_char_lossy().is_whitespace() {
                0 // Whitespace
            } else if big_word {
                1 // Non-whitespace (Big Word)
            } else if c.to_char_lossy().is_alphanumeric() || c.to_char_lossy() == '_' {
                2 // Alphanumeric
            } else {
                3 // Symbol
            }
        };

        self.move_left();

        // 1. Skip whitespace backwards
        while self.cursor > 0 {
            match self.char_at(self.cursor) {
                Some(c) if get_cat(c) == 0 => {
                    self.move_left();
                }
                _ => break,
            }
        }

        // 2. Find start of current category
        if let Some(curr) = self.char_at(self.cursor) {
            let target_cat = get_cat(curr);
            if target_cat == 0 {
                // Still whitespace? Means start of file is whitespace
                return true;
            }

            while self.cursor > 0 {
                let prev_pos = self.cursor - 1;
                match self.char_at(prev_pos) {
                    Some(pc) if get_cat(pc) == target_cat => {
                        self.move_left();
                    }
                    _ => break,
                }
            }
        }

        self.cursor != start_pos
    }

    // ... move_paragraph, move_sentence ...
    // Reuse existing logic but adapt to Character. Use simple stub for now if complex?
    // User wants "Cascading type errors...". I should try to keep functionality.

    pub fn move_paragraph_forward(&mut self) -> bool {
        let current_line = self.get_line();
        let total_lines = self.get_total_lines();
        let start_pos = self.cursor;

        let mut target_line = current_line + 1;
        while target_line < total_lines {
            let start = self.line_index.get_start(target_line).unwrap_or(0);
            let end = self
                .line_index
                .get_end(target_line, self.len())
                .unwrap_or(self.len());

            // If end <= start, line contains only newline (or is empty last line)
            if end <= start {
                self.cursor = start;
                return true;
            }
            target_line += 1;
        }

        // If no empty line found, move to end
        self.move_to_end();
        self.cursor != start_pos
    }

    pub fn move_paragraph_backward(&mut self) -> bool {
        let current_line = self.get_line();
        let start_pos = self.cursor;

        if current_line == 0 {
            self.cursor = 0;
            return self.cursor != start_pos;
        }

        let mut target_line = current_line - 1;
        while target_line > 0 {
            let start = self.line_index.get_start(target_line).unwrap_or(0);
            let end = self
                .line_index
                .get_end(target_line, self.len())
                .unwrap_or(self.len());

            if end <= start {
                self.cursor = start;
                return true;
            }
            target_line -= 1;
        }

        // If no empty line found, move to start
        self.move_to_start();
        self.cursor != start_pos
    }

    pub fn move_sentence_forward(&mut self) -> bool {
        let len = self.len();
        if self.cursor >= len {
            return false;
        }
        let start_pos = self.cursor;

        // Helper to check for sentence terminator
        let is_terminator = |c: Character| matches!(c, Character::Unicode('.' | '!' | '?'));
        let is_whitespace = |c: Character| {
            matches!(c, Character::Unicode(ch) if ch.is_whitespace())
                || matches!(c, Character::Tab | Character::Newline)
        };

        let mut pos = self.cursor;
        while pos < len {
            if let Some(c) = self.char_at(pos) {
                if is_terminator(c) {
                    // Spec: "move to next sentence punctuation if any on the current line, else move to the end of the line or next line."

                    // Found punctuation.
                    // If we are just starting, ensure we actually move.
                    if pos == start_pos {
                        // Skip it to find next? No, behavior is usually jump TO end of sentence or START of next.
                        // Impl: Jump to start of next sentence.
                    }

                    // Check if next char is whitespace or EOF (standard sentence definition)
                    let next_pos = pos + 1;
                    if next_pos >= len || self.char_at(next_pos).map_or(true, is_whitespace) {
                        // Found sentence boundary.

                        // Per user spec: "move to next sentence punctuation if any on the current line"
                        // But also "next sentence punctuation".
                        // Let's implement standard "end of sentence" + skip whitespace logic,
                        // but ensure we handle newlines as soft barriers if requested?
                        // User said: "else move to the end of the line or next line".

                        // Current logic: skips whitespace to find start of NEXT sentence.
                        pos = next_pos;
                        while pos < len {
                            if let Some(wc) = self.char_at(pos) {
                                if !is_whitespace(wc) {
                                    break;
                                }
                            }
                            pos += 1;
                        }
                        self.cursor = pos;
                        return true;
                    }
                } else if c == Character::Newline {
                    // User spec: "else move to the end of the line or next line"
                    // If we hit a newline and haven't found punctuation yet provided we moved.
                    if pos > start_pos {
                        // We are at newline.
                        // Move past it to start of next line?
                        // "move to the end of the line or next line".
                        // Let's stop AT the start of next line (which is pos + 1).
                        self.cursor = pos + 1;
                        return true;
                    }
                }
            }
            pos += 1;
        }

        self.move_to_end();
        self.cursor != start_pos
    }

    pub fn move_sentence_backward(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let start_pos = self.cursor;

        let is_terminator = |c: Character| matches!(c, Character::Unicode('.' | '!' | '?'));
        let is_whitespace = |c: Character| {
            matches!(c, Character::Unicode(ch) if ch.is_whitespace())
                || matches!(c, Character::Tab | Character::Newline)
        };

        let mut pos = self.cursor.saturating_sub(1);
        while pos > 0 {
            if let Some(c) = self.char_at(pos) {
                if is_terminator(c) {
                    let next_pos = pos + 1;
                    if next_pos < self.len() && self.char_at(next_pos).map_or(false, is_whitespace)
                    {
                        // Found terminator + whitespace.
                        // Skip whitespace after terminator to find start of next sentence (which is BEFORE our cursor)
                        let mut s = next_pos;
                        while s < self.len() {
                            if let Some(wc) = self.char_at(s) {
                                if !is_whitespace(wc) {
                                    break;
                                }
                            }
                            s += 1;
                        }

                        // If this sentence start is strictly before our cursor, it's the target.
                        // Since we are scanning backwards, the first valid one we find is the closest previous sentence.
                        if s < self.cursor {
                            self.cursor = s;
                            return true;
                        }
                    }
                }
            }
            pos -= 1;
        }

        // If no sentence boundary found (or we reached start), move to start
        self.move_to_start();
        self.cursor != start_pos
    }
}

impl Display for TextBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    fn chars(&self, range: Range<usize>) -> impl Iterator<Item = Character> + '_ {
        struct CharIter<'a> {
            buffer: &'a TextBuffer,
            current: usize,
            end: usize,
        }

        impl<'a> Iterator for CharIter<'a> {
            type Item = Character;
            fn next(&mut self) -> Option<Self::Item> {
                if self.current >= self.end {
                    None
                } else {
                    let c = self.buffer.char_at(self.current);
                    if c.is_some() {
                        self.current += 1;
                    }
                    c
                }
            }
        }

        CharIter {
            buffer: self,
            current: range.start,
            end: range.end,
        }
    }

    fn revision(&self) -> u64 {
        self.revision
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "movement_tests.rs"]
mod movement_tests;
