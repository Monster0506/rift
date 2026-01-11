//! Rift – Buffer abstraction layer
//!
//! This module defines the traits for a text buffer abstraction, decoupling
//! the editor core from the storage representation. Implementations may use
//! gap buffers, ropes, piece tables, or other data structures.
//!
//! ## Indexing model
//!
//! All offsets are **code‑point based** using Unicode scalar values (U+XXXX).
//! This is not grapheme clusters; emoji sequences, combining marks, and other
//! multi-code-point constructs count as multiple positions. This is a known
//! limitation; grapheme-aware editing would require a separate API layer.
//!
//! ## Implementor requirement
//!
//! Implementations **must** provide O(log n) or better code‑point to byte
//! offset mapping. This is essential for `set_cursor`, `move_left/right`, and
//! deletion operations. Ropes and piece tables should maintain cumulative
//! code‑point counts per node or segment and use binary search.
//!
//! ## Revision semantics
//!
//! Revision increments only on text mutations (insert, delete) or when a
//! transaction commits. Cursor movement—whether via `set_cursor`, `move_left`,
//! or `move_up`—does **not** increment revision. Navigation is not an edit,
//! keeping undo/redo history clean and preventing pure cursor movement from
//! triggering unnecessary re-renders.
//!
//! ## Bulk operations
//!
//! Paste, undo/redo, and macro replay are implemented using transactions.
//! Multiple edits within a transaction commit atomically as a single revision
//! increment. This ensures logical units of work appear as single entries in
//! undo history and prevents rendering from seeing intermediate states.

use crate::character::Character;
use crate::error::RiftError;
use std::ops::Range;

/// Read‑only view of a document at a specific revision.
pub trait BufferView {
    /// Total number of code‑points in the document.
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of logical lines.
    fn line_count(&self) -> usize;

    /// Code‑point offset of the start of `line` (0‑based).
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

/// Builder for accumulating operations in a transaction.
///
/// Operations do not increment revision until the transaction commits.
/// All offsets are code‑point based.
pub trait TransactionBuilder {
    /// Set cursor to `pos` (code‑point offset).
    fn set_cursor(&mut self, pos: usize) -> Result<(), RiftError>;

    /// Current cursor position (code‑point offset).
    fn cursor(&self) -> usize;

    /// Insert UTF‑8 string at cursor.
    fn insert_str(&mut self, s: &str) -> Result<(), RiftError>;

    /// Delete `count` code‑points before cursor (backspace).
    fn delete_backward(&mut self, count: usize) -> Result<(), RiftError>;

    /// Delete `count` code‑points at cursor (forward delete).
    fn delete_forward(&mut self, count: usize) -> Result<(), RiftError>;

    /// Move cursor left by `chars` code‑points. Returns `true` if moved.
    fn move_left(&mut self, chars: usize) -> bool;

    /// Move cursor right by `chars` code‑points. Returns `true` if moved.
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

    /// Set cursor to `pos` (code‑point offset).
    /// Does not increment revision (navigation is not an edit).
    fn set_cursor(&mut self, pos: usize) -> Result<(), RiftError>;

    /// Current cursor position (code‑point offset).
    fn cursor(&self) -> usize;

    /// Insert UTF‑8 string at cursor. Does not move cursor.
    /// Increments revision by 1.
    fn insert_str(&mut self, s: &str) -> Result<(), RiftError>;

    /// Delete `count` code‑points before cursor (backspace).
    /// Increments revision by 1.
    fn delete_backward(&mut self, count: usize) -> Result<(), RiftError>;

    /// Delete `count` code‑points at cursor (forward delete).
    /// Increments revision by 1.
    fn delete_forward(&mut self, count: usize) -> Result<(), RiftError>;

    /// Move cursor left by `chars` code‑points. Returns `true` if moved.
    /// Does not increment revision (navigation is not an edit).
    fn move_left(&mut self, chars: usize) -> bool;

    /// Move cursor right by `chars` code‑points. Returns `true` if moved.
    /// Does not increment revision (navigation is not an edit).
    fn move_right(&mut self, chars: usize) -> bool;

    /// Move cursor up by `lines` logical lines. Returns `true` if moved.
    /// Does not increment revision (navigation is not an edit).
    fn move_up(&mut self, lines: usize) -> bool;

    /// Move cursor down by `lines` logical lines. Returns `true` if moved.
    /// Does not increment revision (navigation is not an edit).
    fn move_down(&mut self, lines: usize) -> bool;

    /// Execute a batch of operations atomically as a single revision increment.
    ///
    /// The closure receives a `TransactionBuilder` and can perform multiple
    /// edits. If the closure returns `Ok(())`, all changes are committed and
    /// revision increments by exactly 1. If it returns `Err`, changes are
    /// rolled back and revision is unchanged.
    ///
    /// Essential for paste, undo/redo, and macro replay.
    fn transaction<F>(&mut self, f: F) -> Result<(), RiftError>
    where
        F: FnOnce(&mut dyn TransactionBuilder) -> Result<(), RiftError>;

    /// Produce a cheap, immutable snapshot of the buffer at the current revision.
    fn snapshot(&self) -> Self::Snapshot;
}
