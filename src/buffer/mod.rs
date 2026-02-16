//! Text Buffer implementation backed by a Piece Table
//!
//! This module provides a `TextBuffer` that manages text using a piece table data structure.
//! It supports efficient insertion and deletion, and handles line indexing.

use crate::buffer::api::BufferView;
use crate::character::Character;
use crate::error::RiftError;
use std::fmt::{self, Display};

use std::cell::RefCell;

pub mod api;
pub mod byte_map;
pub mod line_cache;
pub mod line_index;
pub mod rope;
use line_cache::LineCache;
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
    /// Cache for regex matching lines
    pub line_cache: RefCell<LineCache>,
    /// Cache for byte offsets of line starts (expensive to compute)
    pub byte_map_cache: RefCell<Option<crate::buffer::byte_map::ByteLineMap>>,
}

impl TextBuffer {
    /// Create a new buffer
    pub fn new(_initial_capacity: usize) -> Result<Self, RiftError> {
        // Capacity is managed by the underlying PieceTable/Vec
        Ok(TextBuffer {
            line_index: LineIndex::new(),
            cursor: 0,
            revision: 0,
            line_cache: RefCell::new(LineCache::new()),
            byte_map_cache: RefCell::new(None),
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

    /// Insert a single Character at the cursor position
    pub fn insert_character(&mut self, ch: Character) -> Result<(), RiftError> {
        self.insert_chars(&[ch])
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

    /// Internal insert helper - exposed for Document
    pub fn insert_chars(&mut self, chars: &[Character]) -> Result<(), RiftError> {
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
    pub fn get_chunk_at_byte(&self, _pos: usize) -> &[u8] {
        // Returned empty as stub; Tree-sitter integration uses `to_logical_bytes` + `parse_with` callback now.
        &[]
    }

    /// Get the logical byte content of the buffer (matching parse tree offsets)
    pub fn to_logical_bytes(&self) -> Vec<u8> {
        self.line_index.table.to_logical_bytes()
    }

    /// Get an iterator over the characters
    pub fn iter(&self) -> crate::buffer::rope::PieceTableIterator<'_> {
        self.line_index.table.iter()
    }

    /// Get an iterator starting at a specific character index
    pub fn iter_at(&self, pos: usize) -> crate::buffer::rope::PieceTableIterator<'_> {
        self.line_index.table.iter_at(pos)
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

    // Movement methods - delegated to movement module

    pub fn move_word_right(&mut self) -> bool {
        crate::movement::buffer::move_word_right(self)
    }

    pub fn move_word_end(&mut self) -> bool {
        crate::movement::buffer::move_word_end(self)
    }

    pub fn move_word_left(&mut self) -> bool {
        crate::movement::buffer::move_word_left(self)
    }

    pub fn move_paragraph_forward(&mut self) -> bool {
        crate::movement::buffer::move_paragraph_forward(self)
    }

    pub fn move_paragraph_backward(&mut self) -> bool {
        crate::movement::buffer::move_paragraph_backward(self)
    }

    pub fn move_sentence_forward(&mut self) -> bool {
        crate::movement::buffer::move_sentence_forward(self)
    }

    pub fn move_sentence_backward(&mut self) -> bool {
        crate::movement::buffer::move_sentence_backward(self)
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

    type CharIter<'a> = crate::buffer::rope::PieceTableIterator<'a>;

    fn iter_at(&self, pos: usize) -> Self::CharIter<'_> {
        self.iter_at(pos)
    }

    type ChunkIter<'a> = crate::buffer::rope::PieceTableChunkIterator<'a>;

    fn iter_chunks_at(&self, pos: usize) -> Self::ChunkIter<'_> {
        self.line_index.table.iter_chunks_at(pos)
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn line_cache(&self) -> Option<&std::cell::RefCell<crate::buffer::line_cache::LineCache>> {
        Some(&self.line_cache)
    }

    fn byte_line_map(
        &self,
    ) -> Option<&std::cell::RefCell<Option<crate::buffer::byte_map::ByteLineMap>>> {
        Some(&self.byte_map_cache)
    }

    fn char_to_byte(&self, char_index: usize) -> usize {
        self.char_to_byte(char_index)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "movement_tests.rs"]
mod movement_tests;
