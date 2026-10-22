//! Edit-tracked anchor endpoints: a byte offset plus insertion bias (sec 7).
//! The edit pipeline calls `on_edit` for every edit so ranges move correctly.

use serde::{Deserialize, Serialize};

/// Insertion bias for a marker when text is inserted exactly at its offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Gravity {
    /// Text inserted exactly at the marker stays to the marker's *right*; the
    /// marker does not move. (Emacs insertion-type nil.)
    Left,
    /// Inserted text pushes the marker right. (Emacs insertion-type t.)
    Right,
}

/// A byte offset that is maintained across edits according to its [`Gravity`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub offset: usize,
    pub(crate) gravity: Gravity,
}

impl Marker {
    #[must_use]
    pub(crate) const fn new(offset: usize, gravity: Gravity) -> Self {
        Self { offset, gravity }
    }

    #[must_use]
    pub(crate) const fn left(offset: usize) -> Self {
        Self::new(offset, Gravity::Left)
    }

    #[must_use]
    pub(crate) const fn right(offset: usize) -> Self {
        Self::new(offset, Gravity::Right)
    }

    /// Update for an edit that replaced bytes [start, `old_end`) with new_end-start
    /// bytes. Modeled as delete-then-insert for correctness across replacements.
    pub(crate) fn on_edit(&mut self, start: usize, old_end: usize, new_end: usize) {
        debug_assert!(start <= old_end, "edit range must be ordered");
        let old_len = old_end - start;
        let new_len = new_end.saturating_sub(start);

        // Delete [start, old_end): inside collapses to start, at/after shifts left.
        let mut p = self.offset;
        if p > start {
            if p < old_end {
                p = start;
            } else {
                p -= old_len;
            }
        }

        // Insert new_len at start: after shifts right, at start moves only if Right.
        if p > start || (p == start && self.gravity == Gravity::Right) {
            p += new_len;
        }

        self.offset = p;
    }
}

#[cfg(test)]
#[path = "marker_tests.rs"]
mod marker_tests;

#[cfg(test)]
#[path = "marker_prop_tests.rs"]
mod marker_prop_tests;
