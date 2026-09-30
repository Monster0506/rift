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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharEdit {
    pub pos: usize,
    pub del: usize,
    pub ins: usize,
}

const CHAR_EDIT_LOG_CAP: usize = 64;

pub struct TextBuffer {
    pub line_index: LineIndex,
    cursor: usize,
    desired_col: Option<usize>,
    pub revision: u64,
    pub line_cache: RefCell<LineCache>,
    pub byte_map_cache: RefCell<Option<crate::buffer::byte_map::ByteLineMap>>,
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
            char_edit_log: Vec::new(),
        }
    }
}

impl TextBuffer {
    pub fn new(_initial_capacity: usize) -> Result<Self, RiftError> {
        Ok(TextBuffer {
            line_index: LineIndex::new(),
            cursor: 0,
            desired_col: None,
            revision: 0,
            line_cache: RefCell::new(LineCache::new()),
            byte_map_cache: RefCell::new(None),
            char_edit_log: Vec::new(),
        })
    }

    fn log_char_edit(&mut self, pos: usize, del: usize, ins: usize) {
        if self.char_edit_log.len() < CHAR_EDIT_LOG_CAP {
            self.char_edit_log.push(CharEdit { pos, del, ins });
        }
    }

    pub fn take_char_edits(&mut self) -> Vec<CharEdit> {
        std::mem::take(&mut self.char_edit_log)
    }

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

    #[must_use]
    pub fn len(&self) -> usize {
        self.line_index.len()
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.line_index.table.byte_len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.line_index.is_empty()
    }

    fn col_on_line(&self, line: usize) -> usize {
        let line_start = self.line_index.get_start(line).unwrap_or(0);
        self.cursor.saturating_sub(line_start)
    }

    pub fn get_col(&self) -> usize {
        self.col_on_line(self.get_line())
    }

    pub fn desired_col(&self) -> Option<usize> {
        self.desired_col
    }

    pub fn latch_desired_col(&mut self, col: usize) -> usize {
        *self.desired_col.get_or_insert(col)
    }

    pub fn clear_desired_col(&mut self) {
        self.desired_col = None;
    }

    fn char_pos_for_col(&self, line: usize, col: usize) -> usize {
        let line_start = self.line_index.get_start(line).unwrap_or(0);
        let line_end = self
            .line_index
            .get_end(line, self.len())
            .unwrap_or(self.len());
        let line_len = line_end.saturating_sub(line_start);
        let clamped = col.min(line_len.saturating_sub(1));
        line_start + clamped
    }

    pub fn move_left(&mut self) -> bool {
        self.desired_col = None;
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

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

    pub fn insert(&mut self, byte: u8) -> Result<(), RiftError> {
        let ch = Character::from(byte);
        self.insert_chars(&[ch])
    }

    pub fn insert_char(&mut self, ch: char) -> Result<(), RiftError> {
        let character = Character::from(ch);
        self.insert_chars(&[character])
    }

    pub fn insert_character(&mut self, ch: Character) -> Result<(), RiftError> {
        self.insert_chars(&[ch])
    }

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

    pub fn insert_str(&mut self, s: &str) -> Result<(), RiftError> {
        let chars: Vec<Character> = s.chars().map(Character::from).collect();
        self.insert_chars(&chars)
    }

    pub fn insert_chars(&mut self, chars: &[Character]) -> Result<(), RiftError> {
        let _ins_bytes: usize = chars.iter().map(|c| c.len_utf8()).sum();
        crate::perf_span!(
            "buffer_mutate",
            crate::perf::PerfFields {
                tag: Some("insert"),
                bytes: Some(_ins_bytes as u32),
                ..Default::default()
            }
        );
        self.log_char_edit(self.cursor, 0, chars.len());
        self.line_index.insert(self.cursor, chars);
        self.cursor += chars.len();
        self.revision += 1;
        Ok(())
    }

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
        let _del_bytes = self.char_to_byte(end) - byte_pos;
        crate::perf_span!(
            "buffer_mutate",
            crate::perf::PerfFields {
                tag: Some("delete"),
                bytes: Some(_del_bytes as u32),
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
        true
    }

    pub fn replace_range(&mut self, start: usize, count: usize, chars: &[Character]) -> bool {
        let Some(end) = start.checked_add(count) else {
            return false;
        };
        if end > self.len() {
            return false;
        }
        let byte_pos = self.char_to_byte(start);
        let _del_bytes = self.char_to_byte(end) - byte_pos;
        let _ins_bytes: usize = chars.iter().map(|c| c.len_utf8()).sum();
        crate::perf_span!(
            "buffer_mutate",
            crate::perf::PerfFields {
                tag: Some("replace"),
                bytes: Some((_del_bytes + _ins_bytes) as u32),
                ..Default::default()
            }
        );
        self.log_char_edit(start, count, chars.len());
        self.line_index.replace(start, count, chars);
        self.cursor = start + chars.len();
        self.revision += 1;
        true
    }

    pub fn delete_backward(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.log_char_edit(self.cursor, 1, 0);
        self.line_index.delete(self.cursor, 1);
        self.revision += 1;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        if self.cursor >= self.len() {
            return false;
        }
        self.log_char_edit(self.cursor, 1, 0);
        self.line_index.delete(self.cursor, 1);
        self.revision += 1;
        true
    }

    #[must_use]
    pub fn get_line(&self) -> usize {
        self.line_index.get_line_at(self.cursor)
    }

    #[must_use]
    pub fn get_total_lines(&self) -> usize {
        self.line_index.line_count()
    }

    pub fn byte_to_char(&self, byte_offset: usize) -> usize {
        self.line_index.byte_to_char(byte_offset)
    }

    pub fn char_to_byte(&self, char_index: usize) -> usize {
        self.line_index.char_to_byte(char_index)
    }

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

    pub fn get_chunk_at_byte(&self, _pos: usize) -> &[u8] {
        &[]
    }

    pub fn to_logical_bytes(&self) -> Vec<u8> {
        self.line_index.table.to_logical_bytes()
    }

    pub fn patch_logical_bytes(
        &self,
        cached: &[u8],
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) -> Option<Vec<u8>> {
        if start_byte > old_end_byte || old_end_byte > cached.len() || new_end_byte < start_byte {
            return None;
        }

        let start_char = self.byte_to_char(start_byte);
        let new_end_char = self.byte_to_char(new_end_byte);
        if new_end_char < start_char
            || self.char_to_byte(start_char) != start_byte
            || self.char_to_byte(new_end_char) != new_end_byte
        {
            return None;
        }

        let mut replacement = Vec::with_capacity(new_end_byte - start_byte);
        for ch in self.iter_at(start_char).take(new_end_char - start_char) {
            ch.encode_utf8(&mut replacement);
        }
        if replacement.len() != new_end_byte - start_byte {
            return None;
        }

        let mut patched =
            Vec::with_capacity(start_byte + replacement.len() + cached.len() - old_end_byte);
        patched.extend_from_slice(&cached[..start_byte]);
        patched.extend_from_slice(&replacement);
        patched.extend_from_slice(&cached[old_end_byte..]);

        if patched.len() != self.byte_len() {
            return None;
        }

        Some(patched)
    }

    pub fn iter(&self) -> crate::buffer::rope::PieceTableIterator<'_> {
        self.line_index.table.iter()
    }

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

    pub(crate) fn check_invariants(&self, tier: crate::invariants::InvariantTier) -> Vec<String> {
        use crate::invariants::InvariantTier;

        let mut out = Vec::new();
        let len = self.len();
        if self.cursor > len {
            out.push(format!("cursor {} > buffer len {}", self.cursor, len));
        }

        let byte_len = self.byte_len();
        let cursor_byte = self.char_to_byte(len);
        if cursor_byte != byte_len {
            out.push(format!(
                "char_to_byte(len())={cursor_byte} != byte_len()={byte_len}"
            ));
        }

        if self.cursor <= len {
            let total_lines = self.get_total_lines();
            let line = self.line_index.get_line_at(self.cursor);
            if line >= total_lines {
                out.push(format!(
                    "get_line_at(cursor)={line} >= get_total_lines()={total_lines}"
                ));
            } else if let Some(start) = self.line_index.get_start(line) {
                if start > self.cursor {
                    out.push(format!(
                        "get_line_at(cursor={})={line} but get_start({line})={start} > cursor",
                        self.cursor
                    ));
                }
            }
        }

        if tier >= InvariantTier::Standard {
            out.extend(self.line_index.check_invariants());
        }
        if tier >= InvariantTier::Deep {
            out.extend(self.line_index.table.check_invariants());
        }
        out
    }

    pub fn move_up(&mut self) -> bool {
        let current_line = self.get_line();
        if current_line == 0 {
            return false;
        }
        let col = self.latch_desired_col(self.col_on_line(current_line));
        self.cursor = self.char_pos_for_col(current_line - 1, col);
        true
    }

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

    pub fn move_to_start(&mut self) {
        self.desired_col = None;
        self.cursor = 0;
    }

    pub fn move_to_end(&mut self) {
        self.desired_col = None;
        self.cursor = self.len();
    }

    pub fn move_to_line_start(&mut self) {
        self.desired_col = None;
        let line = self.get_line();
        if let Some(start) = self.line_index.get_start(line) {
            self.cursor = start;
        }
    }

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
