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

/// A single mutation recorded by the buffer, expressed in byte coordinates.
#[derive(Debug, Clone)]
pub struct ByteEdit {
    pub byte_pos: usize,
    pub del_bytes: usize,
    pub ins_bytes: usize,
}

/// A mutation in character coordinates: `del` chars removed at `pos`, then
/// `ins` chars inserted there. Consumed by incremental caches.
#[derive(Debug, Clone, Copy)]
pub struct CharEdit {
    pub pos: usize,
    pub del: usize,
    pub ins: usize,
}

/// Consumers only fast-path a single pending edit; past this cap they
/// rebuild from scratch anyway, so stop accumulating.
const CHAR_EDIT_LOG_CAP: usize = 64;

/// Text buffer using a Piece Table for efficient insertion and deletion.
pub struct TextBuffer {
    /// Line index which also holds the PieceTable
    pub line_index: LineIndex,
    /// Cursor position (Character index)
    cursor: usize,
    /// Target column for vertical motion. None = use real col, MAX = always EOL.
    /// Latched on the first j/k, cleared by h/l/w/0/$ and jumps.
    desired_col: Option<usize>,
    /// Monotonic revision counter for change detection
    pub revision: u64,
    /// Cache for regex matching lines
    pub line_cache: RefCell<LineCache>,
    /// Cache for byte offsets of line starts (expensive to compute)
    pub byte_map_cache: RefCell<Option<crate::buffer::byte_map::ByteLineMap>>,
    pub edit_log: Vec<ByteEdit>,
    /// Char-coordinate mirror of `edit_log`, drained by `take_char_edits`.
    char_edit_log: Vec<CharEdit>,
}

impl Clone for TextBuffer {
    fn clone(&self) -> Self {
        TextBuffer {
            line_index: self.line_index.clone(),
            cursor: self.cursor,
            desired_col: self.desired_col,
            revision: self.revision,
            line_cache: self.line_cache.clone(),
            byte_map_cache: self.byte_map_cache.clone(),
            edit_log: self.edit_log.clone(),
            // A clone starts a new edit lineage; stale char edits must not
            // patch caches keyed to another buffer's history.
            char_edit_log: Vec::new(),
        }
    }
}

impl TextBuffer {
    /// Create a new buffer
    pub fn new(_initial_capacity: usize) -> Result<Self, RiftError> {
        // Capacity is managed by the underlying PieceTable/Vec
        Ok(TextBuffer {
            line_index: LineIndex::new(),
            cursor: 0,
            desired_col: None,
            revision: 0,
            line_cache: RefCell::new(LineCache::new()),
            byte_map_cache: RefCell::new(None),
            edit_log: Vec::new(),
            char_edit_log: Vec::new(),
        })
    }

    fn log_char_edit(&mut self, pos: usize, del: usize, ins: usize) {
        if self.char_edit_log.len() < CHAR_EDIT_LOG_CAP {
            self.char_edit_log.push(CharEdit { pos, del, ins });
        }
    }

    /// Drain the char-coordinate edits logged since the last call.
    pub fn take_char_edits(&mut self) -> Vec<CharEdit> {
        std::mem::take(&mut self.char_edit_log)
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

    /// Total UTF-8 byte length of the text (O(1), from rope metadata).
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.line_index.table.byte_len()
    }

    /// Check if buffer is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.line_index.is_empty()
    }

    /// Compute the column offset of the cursor from the start of its line.
    fn col_on_line(&self, line: usize) -> usize {
        let line_start = self.line_index.get_start(line).unwrap_or(0);
        self.cursor.saturating_sub(line_start)
    }

    /// Column offset of the cursor from the start of its current line.
    pub fn get_col(&self) -> usize {
        self.col_on_line(self.get_line())
    }

    /// Returns the current desired column (None means "use real col").
    pub fn desired_col(&self) -> Option<usize> {
        self.desired_col
    }

    /// If desired_col is unset, sets it to `col` and returns `col`.
    /// If already set, returns the existing value unchanged.
    pub fn latch_desired_col(&mut self, col: usize) -> usize {
        *self.desired_col.get_or_insert(col)
    }

    /// Clears desired_col; called by horizontal motions and jumps.
    pub fn clear_desired_col(&mut self) {
        self.desired_col = None;
    }

    /// Char position on `line` at logical `col`, clamped to the last visible char.
    /// Handles col == usize::MAX (from $) safely; never lands on a trailing newline.
    fn char_pos_for_col(&self, line: usize, col: usize) -> usize {
        let line_start = self.line_index.get_start(line).unwrap_or(0);
        let line_end = self
            .line_index
            .get_end(line, self.len())
            .unwrap_or(self.len());
        // line_end for non-last lines points to the '\n'; exclude it so the
        // cursor never lands on a newline character during vertical motion.
        let line_len = line_end.saturating_sub(line_start);
        let clamped = col.min(line_len.saturating_sub(1));
        line_start + clamped
    }

    /// Move cursor left by one Character.
    pub fn move_left(&mut self) -> bool {
        self.desired_col = None;
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    /// Move cursor right by one Character.
    pub fn move_right(&mut self) -> bool {
        self.desired_col = None;
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
        let mut chars = Vec::with_capacity(bytes.len());
        let mut remaining = bytes;
        loop {
            match std::str::from_utf8(remaining) {
                Ok(s) => {
                    for c in s.chars() {
                        chars.push(Character::from(c));
                    }
                    break;
                }
                Err(e) => {
                    let valid_up_to = e.valid_up_to();
                    // SAFETY: from_utf8 guarantees remaining[..valid_up_to] is valid UTF-8
                    let valid = unsafe { std::str::from_utf8_unchecked(&remaining[..valid_up_to]) };
                    for c in valid.chars() {
                        chars.push(Character::from(c));
                    }
                    let error_len = e.error_len().unwrap_or(1);
                    for &b in &remaining[valid_up_to..valid_up_to + error_len] {
                        chars.push(Character::Byte(b));
                    }
                    remaining = &remaining[valid_up_to + error_len..];
                }
            }
        }
        self.insert_chars(&chars)
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) -> Result<(), RiftError> {
        let chars: Vec<Character> = s.chars().map(Character::from).collect();
        self.insert_chars(&chars)
    }

    /// Internal insert helper - exposed for Document
    pub fn insert_chars(&mut self, chars: &[Character]) -> Result<(), RiftError> {
        let byte_pos = self.char_to_byte(self.cursor);
        let ins_bytes: usize = chars.iter().map(|c| c.len_utf8()).sum();
        crate::perf_span!(
            "buffer_mutate",
            crate::perf::PerfFields {
                tag: Some("insert"),
                bytes: Some(ins_bytes as u32),
                ..Default::default()
            }
        );
        self.log_char_edit(self.cursor, 0, chars.len());
        self.line_index.insert(self.cursor, chars);
        self.cursor += chars.len();
        self.revision += 1;
        self.edit_log.push(ByteEdit {
            byte_pos,
            del_bytes: 0,
            ins_bytes,
        });
        Ok(())
    }

    /// Delete `count` characters starting at `start`, in a single rope operation.
    ///
    /// This is O(log N) vs O(N log N) for a character-by-character loop.
    /// Cursor is clamped: if it was inside the deleted region it moves to `start`;
    /// if it was after the region it shifts back by `count`.
    pub fn delete_range(&mut self, start: usize, count: usize) -> bool {
        if count == 0 {
            return false;
        }
        let Some(end) = start.checked_add(count) else {
            return false;
        };
        if end > self.len() {
            return false;
        }
        let byte_pos = self.char_to_byte(start);
        let del_bytes = self.char_to_byte(end) - byte_pos;
        crate::perf_span!(
            "buffer_mutate",
            crate::perf::PerfFields {
                tag: Some("delete"),
                bytes: Some(del_bytes as u32),
                ..Default::default()
            }
        );
        self.log_char_edit(start, count, 0);
        self.line_index.delete(start, count);
        if self.cursor >= end {
            self.cursor -= count;
        } else if self.cursor > start {
            self.cursor = start;
        }
        self.revision += 1;
        self.edit_log.push(ByteEdit {
            byte_pos,
            del_bytes,
            ins_bytes: 0,
        });
        true
    }

    /// Replace `count` characters at `start` with `chars` in a single rope pass.
    /// Cursor is moved to `start + chars.len()` after the replace.
    pub fn replace_range(&mut self, start: usize, count: usize, chars: &[Character]) -> bool {
        let Some(end) = start.checked_add(count) else {
            return false;
        };
        if end > self.len() {
            return false;
        }
        let byte_pos = self.char_to_byte(start);
        let del_bytes = self.char_to_byte(end) - byte_pos;
        let ins_bytes: usize = chars.iter().map(|c| c.len_utf8()).sum();
        crate::perf_span!(
            "buffer_mutate",
            crate::perf::PerfFields {
                tag: Some("replace"),
                bytes: Some((del_bytes + ins_bytes) as u32),
                ..Default::default()
            }
        );
        self.log_char_edit(start, count, chars.len());
        self.line_index.replace(start, count, chars);
        self.cursor = start + chars.len();
        self.revision += 1;
        self.edit_log.push(ByteEdit {
            byte_pos,
            del_bytes,
            ins_bytes,
        });
        true
    }

    /// Delete the Character before the cursor.
    pub fn delete_backward(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        let byte_pos = self.char_to_byte(self.cursor);
        let del_bytes = self.line_index.char_at(self.cursor).len_utf8();
        self.log_char_edit(self.cursor, 1, 0);
        self.line_index.delete(self.cursor, 1);
        self.revision += 1;
        self.edit_log.push(ByteEdit {
            byte_pos,
            del_bytes,
            ins_bytes: 0,
        });
        true
    }

    /// Delete the Character at the cursor position.
    pub fn delete_forward(&mut self) -> bool {
        if self.cursor >= self.len() {
            return false;
        }
        let target = self.line_index.char_at(self.cursor);
        let byte_pos = self.char_to_byte(self.cursor);
        let del_bytes = target.len_utf8();
        self.log_char_edit(self.cursor, 1, 0);
        self.line_index.delete(self.cursor, 1);
        self.revision += 1;
        self.edit_log.push(ByteEdit {
            byte_pos,
            del_bytes,
            ins_bytes: 0,
        });
        true
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
    pub fn get_chunk_at_byte(&self, _pos: usize) -> &[u8] {
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

    /// Move cursor up one line, preserving desired_col across short lines.
    pub fn move_up(&mut self) -> bool {
        let current_line = self.get_line();
        if current_line == 0 {
            return false;
        }
        let col = self.latch_desired_col(self.col_on_line(current_line));
        self.cursor = self.char_pos_for_col(current_line - 1, col);
        true
    }

    /// Move cursor down one line, preserving desired_col across short lines.
    pub fn move_down(&mut self) -> bool {
        let current_line = self.get_line();
        let total_lines = self.get_total_lines();
        if current_line + 1 >= total_lines {
            return false;
        }
        let col = self.latch_desired_col(self.col_on_line(current_line));
        self.cursor = self.char_pos_for_col(current_line + 1, col);
        true
    }

    /// Move to start of buffer.
    pub fn move_to_start(&mut self) {
        self.desired_col = None;
        self.cursor = 0;
    }

    /// Move to end of buffer.
    pub fn move_to_end(&mut self) {
        self.desired_col = None;
        self.cursor = self.len();
    }

    /// Move to start of current line (0 / ^).
    pub fn move_to_line_start(&mut self) {
        self.desired_col = None;
        let line = self.get_line();
        if let Some(start) = self.line_index.get_start(line) {
            self.cursor = start;
        }
    }

    /// Move to end of current line ($). Sets desired_col = MAX so subsequent
    /// vertical moves always land at EOL regardless of line length.
    pub fn move_to_line_end(&mut self) {
        self.desired_col = Some(usize::MAX);
        let line = self.get_line();
        if let Some(end) = self.line_index.get_end(line, self.len()) {
            self.cursor = end;
        }
    }

    pub fn move_word_right(&mut self) -> bool {
        self.desired_col = None;
        crate::movement::buffer::move_word_right(self)
    }

    pub fn move_word_end(&mut self) -> bool {
        self.desired_col = None;
        crate::movement::buffer::move_word_end(self)
    }

    pub fn move_word_left(&mut self) -> bool {
        self.desired_col = None;
        crate::movement::buffer::move_word_left(self)
    }

    pub fn move_big_word_right(&mut self) -> bool {
        self.desired_col = None;
        crate::movement::buffer::move_big_word_right(self)
    }

    pub fn move_big_word_left(&mut self) -> bool {
        self.desired_col = None;
        crate::movement::buffer::move_big_word_left(self)
    }

    pub fn move_paragraph_forward(&mut self) -> bool {
        self.desired_col = None;
        crate::movement::buffer::move_paragraph_forward(self)
    }

    pub fn move_paragraph_backward(&mut self) -> bool {
        self.desired_col = None;
        crate::movement::buffer::move_paragraph_backward(self)
    }

    pub fn move_sentence_forward(&mut self) -> bool {
        self.desired_col = None;
        crate::movement::buffer::move_sentence_forward(self)
    }

    pub fn move_sentence_backward(&mut self) -> bool {
        self.desired_col = None;
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
