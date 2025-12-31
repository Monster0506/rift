//! Wrapper around PieceTable to replace the old LineIndex
//!
//! This struct now serves as the primary storage and indexing engine,
//! though for now it maintains compatibility with the old LineIndex API
//! where possible.

use super::rope::PieceTable;

pub struct LineIndex {
    pub table: PieceTable,
}

impl LineIndex {
    pub fn new() -> Self {
        Self {
            table: PieceTable::new(Vec::new()),
        }
    }

    pub fn line_count(&self) -> usize {
        self.table.get_line_count()
    }

    pub fn get_start(&self, line_idx: usize) -> Option<usize> {
        if line_idx >= self.table.get_line_count() {
            return None;
        }
        Some(self.table.line_start_offset(line_idx))
    }

    pub fn get_line_start(&self, line_idx: usize) -> usize {
        self.table.line_start_offset(line_idx)
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
        // Note: This assumes standard \n line endings.
        let next_start = self.table.line_start_offset(line_idx + 1);
        Some(next_start.saturating_sub(1))
    }

    pub fn get_line_at(&self, pos: usize) -> usize {
        self.table.line_at_byte(pos)
    }

    pub fn insert(&mut self, pos: usize, bytes: &[u8]) {
        self.table.insert(pos, bytes);
    }

    pub fn delete(&mut self, pos: usize, len: usize) {
        self.table.delete(pos..pos + len);
    }

    // New methods to expose text access
    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub fn byte_at(&self, pos: usize) -> u8 {
        self.table.byte_at(pos)
    }

    pub fn bytes_range(&self, range: std::ops::Range<usize>) -> Vec<u8> {
        self.table.bytes_range(range)
    }

    pub fn chunks_in_range(&self, range: std::ops::Range<usize>) -> impl Iterator<Item = &[u8]> {
        self.table.chunks_in_range(range)
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
