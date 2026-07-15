//! Buffer abstraction traits decoupling the editor core from storage. Offsets
//! are code-point based; revision increments only on mutation/commit, never on cursor movement.

use crate::character::Character;
use crate::error::RiftError;
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

    /// Access to byte line map cache if available (for search index optimization)
    fn byte_line_map(
        &self,
    ) -> Option<&std::cell::RefCell<Option<crate::buffer::byte_map::ByteLineMap>>> {
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

/// Builder for accumulating operations in a transaction. Code-point based
/// offsets; operations do not increment revision until the transaction commits.
pub trait TransactionBuilder {
    /// Set cursor to `pos` (code-point offset).
    fn set_cursor(&mut self, pos: usize) -> Result<(), RiftError>;

    /// Current cursor position (code-point offset).
    fn cursor(&self) -> usize;

    /// Insert UTF-8 string at cursor.
    fn insert_str(&mut self, s: &str) -> Result<(), RiftError>;

    /// Delete `count` code-points before cursor (backspace).
    fn delete_backward(&mut self, count: usize) -> Result<(), RiftError>;

    /// Delete `count` code-points at cursor (forward delete).
    fn delete_forward(&mut self, count: usize) -> Result<(), RiftError>;

    /// Move cursor left by `chars` code-points. Returns `true` if moved.
    fn move_left(&mut self, chars: usize) -> bool;

    /// Move cursor right by `chars` code-points. Returns `true` if moved.
    fn move_right(&mut self, chars: usize) -> bool;

    /// Move cursor up by `lines` logical lines. Returns `true` if moved.
    fn move_up(&mut self, lines: usize) -> bool;

    /// Move cursor down by `lines` logical lines. Returns `true` if moved.
    fn move_down(&mut self, lines: usize) -> bool;
}

/// Mutable buffer interface used by the command executor and UI.
pub trait BufferMut {
    /// The concrete snapshot type for this buffer.
    type Snapshot: BufferView + Send + Sync;

    /// Set cursor to `pos` (code-point offset).
    /// Does not increment revision (navigation is not an edit).
    fn set_cursor(&mut self, pos: usize) -> Result<(), RiftError>;

    /// Current cursor position (code-point offset).
    fn cursor(&self) -> usize;

    /// Insert UTF-8 string at cursor. Does not move cursor.
    /// Increments revision by 1.
    fn insert_str(&mut self, s: &str) -> Result<(), RiftError>;

    /// Delete `count` code-points before cursor (backspace).
    /// Increments revision by 1.
    fn delete_backward(&mut self, count: usize) -> Result<(), RiftError>;

    /// Delete `count` code-points at cursor (forward delete).
    /// Increments revision by 1.
    fn delete_forward(&mut self, count: usize) -> Result<(), RiftError>;

    /// Move cursor left by `chars` code-points. Returns `true` if moved.
    /// Does not increment revision (navigation is not an edit).
    fn move_left(&mut self, chars: usize) -> bool;

    /// Move cursor right by `chars` code-points. Returns `true` if moved.
    /// Does not increment revision (navigation is not an edit).
    fn move_right(&mut self, chars: usize) -> bool;

    /// Move cursor up by `lines` logical lines. Returns `true` if moved.
    /// Does not increment revision (navigation is not an edit).
    fn move_up(&mut self, lines: usize) -> bool;

    /// Move cursor down by `lines` logical lines. Returns `true` if moved.
    /// Does not increment revision (navigation is not an edit).
    fn move_down(&mut self, lines: usize) -> bool;

    /// Execute a batch of operations atomically as a single revision increment,
    /// or rolls back with revision unchanged if the closure returns `Err`.
    fn transaction<F>(&mut self, f: F) -> Result<(), RiftError>
    where
        F: FnOnce(&mut dyn TransactionBuilder) -> Result<(), RiftError>;

    /// Produce a cheap, immutable snapshot of the buffer at the current revision.
    fn snapshot(&self) -> Self::Snapshot;
}
