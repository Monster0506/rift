



use super::{Document, GhostCut};
use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind, Presentation, StyleOverride};
use crate::buffer::api::BufferView;
use crate::character::Character;
use crate::color::Color;

const GHOST_PRIORITY: i32 = 8;

impl Document {
    
    pub fn create_ghosts(&mut self, ranges: &[(usize, usize)]) {
        self.commit_pending_ghost();

        let mut ranges: Vec<(usize, usize)> =
            ranges.iter().copied().filter(|&(s, e)| s < e).collect();
        if ranges.is_empty() {
            return;
        }
        ranges.sort_unstable_by_key(|&(start, _)| std::cmp::Reverse(start));

        let cursor_before = self.buffer.cursor();
        self.begin_transaction("Delete");
        let mut ghosts = Vec::with_capacity(ranges.len());
        for &(start, end) in &ranges {
            let text: Vec<Character> = self.buffer.chars(start..end).collect();
            let _ = self.delete_range(start, end);
            ghosts.push(GhostCut {
                at: start,
                text,
                painted: None,
            });
        }
        self.commit_transaction();

        let shift: usize = ranges
            .iter()
            .filter(|&&(start, _)| start <= cursor_before)
            .map(|&(start, end)| end.min(cursor_before) - start)
            .sum();
        let restored = cursor_before.saturating_sub(shift).min(self.buffer.len());
        let _ = self.buffer.set_cursor(restored);

        ghosts.sort_unstable_by_key(|g| g.at);
        self.pending_ghost = ghosts;
    }

    pub fn most_recent_ghost(&self) -> Option<(&[Character], usize)> {
        let ghost = self.pending_ghost.first()?;
        Some((&ghost.text, ghost.at))
    }

    pub fn commit_pending_ghost(&mut self) {
        if self.pending_ghost.is_empty() {
            return;
        }
        self.end_ghost_paint();
        self.pending_ghost.clear();
    }

    pub fn begin_ghost_paint(&mut self) -> Option<super::SyntaxSync> {
        if self.pending_ghost.is_empty() {
            return None;
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

            let _ = self.buffer.set_cursor(insert_at);
            self.ghost_paint_active = true;
            let _ = self.insert_characters(&text);
            self.ghost_paint_active = false;

            let byte_start = self.buffer.char_to_byte(insert_at);
            let byte_end = self.buffer.char_to_byte(insert_at + len);
            let annotation_id = self.annotations.add(
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

        let _ = self
            .buffer
            .set_cursor((real_cursor + cursor_shift).min(self.buffer.len()));
        Some(self.resync_syntax())
    }

    pub fn end_ghost_paint(&mut self) -> Option<super::SyntaxSync> {
        let real_cursor = self.ghost_paint_cursor.take()?;

        let mut order: Vec<usize> = (0..self.pending_ghost.len())
            .filter(|&i| self.pending_ghost[i].painted.is_some())
            .collect();
        order
            .sort_unstable_by_key(|&i| std::cmp::Reverse(self.pending_ghost[i].painted.unwrap().0));

        for i in order {
            let Some((offset, annotation_id)) = self.pending_ghost[i].painted.take() else {
                continue;
            };
            let len = self.pending_ghost[i].text.len();
            self.annotations.remove(annotation_id);
            self.ghost_paint_active = true;
            let _ = self.delete_range(offset, offset + len);
            self.ghost_paint_active = false;
        }

        let _ = self.buffer.set_cursor(real_cursor.min(self.buffer.len()));
        Some(self.resync_syntax())
    }
}
