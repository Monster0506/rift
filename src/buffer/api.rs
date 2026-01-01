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

use crate::error::RiftError;

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

    /// Contents of `line` without the trailing newline, as an iterator over
    /// byte slices. Chunks are contiguous in the underlying storage.
    fn line_bytes(&self, line: usize) -> impl Iterator<Item = &[u8]> + '_;

    /// Slice of the document between code‑point offsets `[start, end)`,
    /// as an iterator over byte slices.
    fn slice(&self, start: usize, end: usize) -> impl Iterator<Item = &[u8]> + '_;

    /// Revision identifier; increments on text mutations or transaction commits.
    fn revision(&self) -> u64;
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
