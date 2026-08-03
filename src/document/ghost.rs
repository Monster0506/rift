//! Deferred `d`-cut ("ghost cut"): buffer content stays visible under a
//! greyed-out annotation until something resolves it into a real delete.

use super::{Document, GhostCut};
use crate::annotations::{
    Anchor, Annotation, AnnotationId, AnnotationOwner, Kind, Presentation, StyleOverride,
};
use crate::buffer::api::BufferView;
use crate::character::Character;
use crate::color::Color;

/// Above search (5/10) and selection (4/6) highlight priorities, so a cut
/// always reads as a cut regardless of what else highlights that span.
const GHOST_PRIORITY: i32 = 8;

impl Document {
    /// Ghost-cut `ranges` (char offsets, half-open) as one batch, committing
    /// whatever was already pending in this document first.
    pub fn create_ghosts(&mut self, ranges: &[(usize, usize)]) {
        self.commit_pending_ghost();
        let style = StyleOverride {
            fg: Some(Color::DarkGrey),
            ..Default::default()
        };
        for &(start, end) in ranges {
            if start >= end {
                continue;
            }
            let text: Vec<Character> = self.buffer.chars(start..end).collect();
            let byte_start = self.buffer.char_to_byte(start);
            let byte_end = self.buffer.char_to_byte(end);
            let annotation_id = self.annotations.add(
                Annotation::new(
                    Kind::new("ghostcut.pending"),
                    Anchor::range(byte_start, byte_end),
                    AnnotationOwner::System,
                )
                .with_presentation(Presentation::with_style(style).with_priority(GHOST_PRIORITY)),
            );
            self.pending_ghost.push(GhostCut {
                start,
                end,
                text,
                annotation_id,
            });
        }
    }

    /// The most-recently-created still-pending ghost's text and live start
    /// offset, for Put's same-location-paste check. `None` once resolved.
    pub fn most_recent_ghost(&self) -> Option<(&[Character], usize)> {
        let ghost = self.pending_ghost.last()?;
        let (start, _) = self.ghost_live_range(ghost.annotation_id)?;
        Some((&ghost.text, start))
    }

    /// Live (start, end) char offsets for a pending ghost's annotation.
    fn ghost_live_range(&self, annotation_id: AnnotationId) -> Option<(usize, usize)> {
        let annotation = self.annotations.get(annotation_id)?;
        let Anchor::Range(s, e) = annotation.anchor else {
            return None;
        };
        if s.offset >= e.offset {
            return None;
        }
        Some((
            self.buffer.byte_to_char(s.offset),
            self.buffer.byte_to_char(e.offset),
        ))
    }

    /// Commit every pending ghost as an ordinary delete in one transaction.
    /// Reads each ghost's live annotation position, not its (possibly stale) cached `start`/`end`.
    pub fn commit_pending_ghost(&mut self) {
        let ghosts = std::mem::take(&mut self.pending_ghost);
        if ghosts.is_empty() {
            return;
        }
        let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(ghosts.len());
        for ghost in &ghosts {
            if let Some(range) = self.ghost_live_range(ghost.annotation_id) {
                ranges.push(range);
            }
            self.annotations.remove(ghost.annotation_id);
        }
        if ranges.is_empty() {
            return;
        }
        // Highest-offset-first so each delete's shift never invalidates an
        // earlier range's already-captured live position.
        ranges.sort_unstable_by(|a, b| b.0.cmp(&a.0));

        let cursor_before = self.buffer.cursor();
        self.begin_transaction("Ghost cut");
        for &(start, end) in &ranges {
            let _ = self.delete_range(start, end);
        }
        self.commit_transaction();

        // delete_range leaves the cursor at the last range it processed, which is
        // wrong if the caller moved away without resolving first: restore instead.
        let shift: usize = ranges
            .iter()
            .filter(|&&(start, _)| start <= cursor_before)
            .map(|&(start, end)| end.min(cursor_before) - start)
            .sum();
        let restored = cursor_before.saturating_sub(shift).min(self.buffer.len());
        let _ = self.buffer.set_cursor(restored);
    }
}
