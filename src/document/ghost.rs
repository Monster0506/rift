use super::ops::{Origin, Resolved};
use super::{Document, GhostCut};
use crate::annotations::kind::Kind;
use crate::annotations::presentation::{Presentation, StyleOverride};
use crate::annotations::{Anchor, Annotation, AnnotationOwner};
use crate::buffer::api::BufferView;
use crate::character::Character;
use crate::clipboard::Clip;
use crate::color::Color;

const GHOST_PRIORITY: i32 = 8;

impl super::ops::DocumentWrite<'_> {
    pub(crate) fn create_ghosts(&mut self, ranges: &[(usize, usize)], clip: Clip) -> bool {
        self.doc.create_ghosts(ranges, clip)
    }
}

impl Document {
    fn create_ghosts(&mut self, ranges: &[(usize, usize)], clip: Clip) -> bool {
        self.commit_pending_ghost();

        let mut ranges: Vec<(usize, usize)> =
            ranges.iter().copied().filter(|&(s, e)| s < e).collect();
        if ranges.is_empty() {
            return false;
        }
        ranges.sort_unstable_by_key(|&(start, _)| std::cmp::Reverse(start));

        let cursor_before = self.buffer.cursor();
        self.begin_transaction("Delete");
        let mut ghosts = Vec::with_capacity(ranges.len());
        for &(start, end) in &ranges {
            let text: Vec<Character> = self.buffer.chars(start..end).collect();
            if self
                .apply_edit(
                    super::ops::Edit::delete(start..end),
                    super::ops::EditSource::User,
                )
                .is_ok_and(|changed| changed)
            {
                ghosts.push(GhostCut {
                    at: start,
                    text,
                    painted: None,
                });
            }
        }
        self.commit_transaction();

        if ghosts.is_empty() {
            return false;
        }
        let shift: usize = ghosts
            .iter()
            .filter(|ghost| ghost.at <= cursor_before)
            .map(|ghost| ghost.text.len().min(cursor_before - ghost.at))
            .sum();
        let restored = cursor_before.saturating_sub(shift).min(self.buffer.len());
        let _ = self.set_cursor(restored, crate::document::ops::CursorCause::Programmatic);

        ghosts.sort_unstable_by_key(|g| g.at);
        self.pending_ghost = ghosts;
        self.ghost_clip = Some(clip);
        true
    }

    pub(crate) fn most_recent_ghost(&self) -> Option<(&[Character], usize, &Clip)> {
        let ghost = self.pending_ghost.first()?;
        let clip = self.ghost_clip.as_ref()?;
        Some((&ghost.text, ghost.at, clip))
    }

    pub(crate) fn commit_pending_ghost(&mut self) {
        if self.pending_ghost.is_empty() {
            return;
        }
        self.end_ghost_paint();
        self.pending_ghost.clear();
        self.ghost_clip = None;
    }

    pub(crate) fn begin_ghost_paint(&mut self) {
        if self.pending_ghost.is_empty() {
            return;
        }
        debug_assert!(self.ghost_paint_cursor.is_none(), "ghost paint re-entered");
        let real_cursor = self.buffer.cursor();
        self.ghost_paint_cursor = Some(real_cursor);

        let style = StyleOverride {
            fg: Some(Color::DarkGrey),
            ..Default::default()
        };

        let mut shift = 0usize;
        let mut cursor_shift = 0usize;
        for i in 0..self.pending_ghost.len() {
            let ghost_at = self.pending_ghost[i].at;
            let insert_at = ghost_at + shift;
            let text = self.pending_ghost[i].text.clone();
            let len = text.len();

            if ghost_at < real_cursor {
                cursor_shift += len;
            }

            let _ = self.apply_resolved(
                Resolved {
                    start: insert_at,
                    end: insert_at,
                    text,
                },
                Origin::GhostPaint,
            );

            let byte_start = self.buffer.char_to_byte(insert_at);
            let byte_end = self.buffer.char_to_byte(insert_at + len);
            let annotation_id = self.annotate(
                Annotation::new(
                    Kind::new("ghostcut.pending"),
                    Anchor::range(byte_start, byte_end),
                    AnnotationOwner::System,
                )
                .with_presentation(Presentation::with_style(style).with_priority(GHOST_PRIORITY)),
            );
            self.pending_ghost[i].painted = Some((insert_at, annotation_id));

            shift += len;
        }

        let _ = self.set_cursor(
            (real_cursor + cursor_shift).min(self.buffer.len()),
            crate::document::ops::CursorCause::Programmatic,
        );
        self.settle_syntax();
    }

    pub(crate) fn end_ghost_paint(&mut self) {
        let Some(real_cursor) = self.ghost_paint_cursor.take() else {
            return;
        };

        let mut order: Vec<(usize, usize)> = self
            .pending_ghost
            .iter()
            .enumerate()
            .filter_map(|(i, g)| Some((g.painted?.0, i)))
            .collect();
        order.sort_unstable_by_key(|&(offset, _)| std::cmp::Reverse(offset));

        for (_, i) in order {
            let Some((offset, annotation_id)) = self.pending_ghost[i].painted.take() else {
                continue;
            };
            let len = self.pending_ghost[i].text.len();
            self.unannotate(annotation_id);
            let _ = self.apply_resolved(
                Resolved {
                    start: offset,
                    end: offset + len,
                    text: Vec::new(),
                },
                Origin::GhostPaint,
            );
        }

        let _ = self.set_cursor(
            real_cursor.min(self.buffer.len()),
            crate::document::ops::CursorCause::Programmatic,
        );
        self.settle_syntax();
    }
}
