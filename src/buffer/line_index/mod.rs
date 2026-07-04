//! Wrapper around PieceTable to replace the old LineIndex
//!
//! This struct now serves as the primary storage and indexing engine,
//! though for now it maintains compatibility with the old LineIndex API
//! where possible.

use super::rope::PieceTable;
use crate::character::Character;
use std::cell::RefCell;

#[derive(Clone)]
pub struct LineIndex {
    pub table: PieceTable,
    /// Char offset of the start of each line; built lazily, then maintained
    /// incrementally on insert/delete. `None` means "not built".
    line_starts: RefCell<Option<Vec<usize>>>,
}

impl LineIndex {
    pub fn new() -> Self {
        Self {
            table: PieceTable::new(Vec::new()),
            line_starts: RefCell::new(None),
        }
    }

    /// Wrap an existing piece table (e.g. content loaded by a background job).
    /// The line-start cache builds lazily on first query.
    pub fn from_table(table: PieceTable) -> Self {
        Self {
            table,
            line_starts: RefCell::new(None),
        }
    }

    /// Build the line-start offset vector if it has not been built yet (one
    /// O(n) pass over the buffer). All line queries go through this first.
    fn ensure_built(&self) {
        let mut cache = self.line_starts.borrow_mut();
        if cache.is_some() {
            return;
        }
        let mut starts = Vec::with_capacity(self.table.get_line_count());
        starts.push(0);
        let mut pos = 0usize;
        for ch in self.table.iter() {
            pos += 1;
            if ch == Character::Newline {
                starts.push(pos);
            }
        }
        *cache = Some(starts);
    }

    /// Shift entries after `pos` right by `chars.len()`; insert a new line
    /// start for each newline in `chars`.
    fn apply_insert(starts: &mut Vec<usize>, pos: usize, chars: &[Character]) {
        let added = chars.len();
        if added == 0 {
            return;
        }
        let split = starts.partition_point(|&s| s <= pos);
        for s in starts[split..].iter_mut() {
            *s += added;
        }
        let new_starts: Vec<usize> = chars
            .iter()
            .enumerate()
            .filter(|(_, ch)| **ch == Character::Newline)
            .map(|(k, _)| pos + k + 1)
            .collect();
        if !new_starts.is_empty() {
            starts.splice(split..split, new_starts);
        }
    }

    /// Drop line starts inside `(pos, pos+len]`; shift remaining entries
    /// after the deletion left by `len`.
    fn apply_delete(starts: &mut Vec<usize>, pos: usize, len: usize) {
        if len == 0 {
            return;
        }
        let end = pos + len;
        let first_removed = starts.partition_point(|&s| s <= pos);
        let first_kept = starts.partition_point(|&s| s <= end);
        starts.drain(first_removed..first_kept);
        for s in starts[first_removed..].iter_mut() {
            *s -= len;
        }
    }

    pub fn line_count(&self) -> usize {
        self.table.get_line_count()
    }

    pub fn get_start(&self, line_idx: usize) -> Option<usize> {
        if line_idx >= self.table.get_line_count() {
            return None;
        }
        self.ensure_built();
        Some(self.line_starts.borrow().as_ref().unwrap()[line_idx])
    }

    pub fn get_line_start(&self, line_idx: usize) -> usize {
        self.ensure_built();
        let cache = self.line_starts.borrow();
        let starts = cache.as_ref().unwrap();
        starts
            .get(line_idx)
            .copied()
            .unwrap_or_else(|| self.table.len())
    }

    pub fn get_end(&self, line_idx: usize, total_len: usize) -> Option<usize> {
        if line_idx >= self.table.get_line_count() {
            return None;
        }

        // If it's the last line
        if line_idx + 1 == self.table.get_line_count() {
            return Some(total_len);
        }

        // Otherwise, it's the start of next line - 1 (newline)
        let next_start = self.get_line_start(line_idx + 1);
        Some(next_start.saturating_sub(1))
    }

    pub fn get_line_at(&self, pos: usize) -> usize {
        self.ensure_built();
        let cache = self.line_starts.borrow();
        let starts = cache.as_ref().unwrap();
        // Line of `pos` = index of the last line start <= pos.
        starts.partition_point(|&s| s <= pos).saturating_sub(1)
    }

    pub fn insert(&mut self, pos: usize, chars: &[Character]) {
        self.table.insert(pos, chars);
        if let Some(starts) = self.line_starts.get_mut().as_mut() {
            Self::apply_insert(starts, pos, chars);
            debug_assert_eq!(starts.len(), self.table.get_line_count());
        }
    }

    pub fn delete(&mut self, pos: usize, len: usize) {
        self.table.delete(pos..pos + len);
        if let Some(starts) = self.line_starts.get_mut().as_mut() {
            Self::apply_delete(starts, pos, len);
            debug_assert_eq!(starts.len(), self.table.get_line_count());
        }
    }

    pub fn replace(&mut self, pos: usize, count: usize, chars: &[Character]) {
        self.table.replace(pos, count, chars);
        if let Some(starts) = self.line_starts.get_mut().as_mut() {
            Self::apply_delete(starts, pos, count);
            Self::apply_insert(starts, pos, chars);
            debug_assert_eq!(starts.len(), self.table.get_line_count());
        }
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub fn char_at(&self, pos: usize) -> Character {
        self.table.char_at(pos)
    }

    /// Convert byte offset to character index
    pub fn byte_to_char(&self, byte_offset: usize) -> usize {
        self.table.byte_to_char(byte_offset)
    }

    /// Convert character index to byte offset
    pub fn char_to_byte(&self, char_index: usize) -> usize {
        self.table.char_to_byte(char_index)
    }

    pub fn bytes_range(&self, range: std::ops::Range<usize>) -> Vec<u8> {
        self.table.bytes_range(range)
    }
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LineIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LineIndex")
            .field("lines", &self.line_count())
            .finish()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
