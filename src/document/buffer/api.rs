//! Buffer abstraction traits decoupling the editor core from storage. Offsets
//! are code-point based; revision increments only on mutation/commit, never on cursor movement.

use crate::character::Character;
use std::ops::Range;

/// Read-only view of a document at a specific revision.
pub trait BufferView {
    /// Total number of code-points in the document.
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of logical lines.
    fn line_count(&self) -> usize;

    /// Code-point offset of the start of `line` (0-based).
    fn line_start(&self, line: usize) -> usize;

    type CharIter<'a>: Iterator<Item = Character> + Clone + 'a
    where
        Self: 'a;

    type ChunkIter<'a>: Iterator<Item = &'a [crate::character::Character]> + Clone + 'a
    where
        Self: 'a;

    /// Iterator starting at the given code-point offset.
    fn iter_at(&self, pos: usize) -> Self::CharIter<'_>;

    /// Iterator over chunks starting at the given code-point offset.
    fn iter_chunks_at(&self, pos: usize) -> Self::ChunkIter<'_>;

    /// Characters in the given range.
    fn chars(&self, range: Range<usize>) -> impl Iterator<Item = Character> + '_ {
        self.iter_at(range.start).take(range.end - range.start)
    }

    /// Revision identifier; increments on text mutations or transaction commits.
    fn revision(&self) -> u64;

    /// Access to line cache if available (for Tier 2 search optimization)
    fn line_cache(&self) -> Option<&std::cell::RefCell<crate::buffer::line_cache::LineCache>> {
        None
    }

    /// Convert character index to byte offset (efficient O(log N) or better)
    fn char_to_byte(&self, char_index: usize) -> usize {
        // Default implementation falls back to iteration (slow)
        // Implementors should override this!
        let mut byte_offset = 0;
        for (char_count, c) in self.chars(0..self.len()).enumerate() {
            if char_count == char_index {
                return byte_offset;
            }
            byte_offset += c.len_utf8();
        }
        byte_offset
    }
}
