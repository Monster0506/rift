use super::ops::{CursorCause, Edit, EditSource};
use super::Document;
use crate::annotations::kind::Kind;
use crate::annotations::presentation::{Presentation, StyleOverride};
use crate::annotations::{Anchor, Annotation, AnnotationOwner, AnnotationStore};
use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::clipboard::Clip;
use crate::color::Color;
use std::ops::Range;

struct GhostCut {
    chars: Range<usize>,
    bytes: Range<usize>,
    live_char: usize,
    live_byte: usize,
}

pub(crate) struct GhostPreview {
    pub(crate) buffer: TextBuffer,
    pub(crate) annotations: AnnotationStore,
    cuts: Vec<GhostCut>,
    clip: Clip,
    annotation_revision: Option<u64>,
}

impl GhostPreview {
    pub(crate) fn project_char(&self, pos: usize) -> usize {
        pos + self
            .cuts
            .iter()
            .take_while(|cut| cut.live_char <= pos)
            .map(|cut| cut.chars.len())
            .sum::<usize>()
    }

    pub(crate) fn live_char(&self, pos: usize) -> usize {
        pos - self
            .cuts
            .iter()
            .take_while(|cut| cut.chars.start < pos)
            .map(|cut| cut.chars.end.min(pos) - cut.chars.start)
            .sum::<usize>()
    }

    pub(crate) fn live_byte(&self, pos: usize) -> usize {
        pos - self
            .cuts
            .iter()
            .take_while(|cut| cut.bytes.start < pos)
            .map(|cut| cut.bytes.end.min(pos) - cut.bytes.start)
            .sum::<usize>()
    }

    fn project_byte(&self, pos: usize, after: bool) -> usize {
        pos + self
            .cuts
            .iter()
            .take_while(|cut| cut.live_byte < pos || (after && cut.live_byte == pos))
            .map(|cut| cut.bytes.len())
            .sum::<usize>()
    }

    pub(crate) fn project_bytes(&self, span: Range<usize>) -> Range<usize> {
        self.project_byte(span.start, true)..self.project_byte(span.end, span.is_empty())
    }

    pub(crate) fn project_search(
        &self,
        matches: &[crate::search::SearchMatch],
    ) -> Vec<crate::search::SearchMatch> {
        use crate::search::SearchMatch;
        let mut projected = Vec::with_capacity(matches.len());
        for m in matches {
            let mut start = self.project_char(m.range.start);
            for cut in self
                .cuts
                .iter()
                .filter(|cut| m.range.start < cut.live_char && cut.live_char < m.range.end)
            {
                projected.push(SearchMatch {
                    range: start..cut.chars.start,
                });
                start = cut.chars.end;
            }
            let end = if m.range.is_empty() {
                start
            } else {
                m.range.end
                    + self
                        .cuts
                        .iter()
                        .take_while(|cut| cut.live_char < m.range.end)
                        .map(|cut| cut.chars.len())
                        .sum::<usize>()
            };
            projected.push(SearchMatch { range: start..end });
        }
        projected
    }

    fn refresh(&mut self, live: &TextBuffer, annotations: &AnnotationStore) {
        let _ = self.buffer.set_cursor(self.project_char(live.cursor()));
        if self.annotation_revision == Some(annotations.revision()) {
            return;
        }
        let mut projected = AnnotationStore::new();
        for source in annotations.iter() {
            let mut annotation = source.clone();
            match &mut annotation.anchor {
                Anchor::Point(point) => point.offset = self.project_byte(point.offset, true),
                Anchor::Range(start, end) => {
                    let range = self.project_bytes(start.offset..end.offset);
                    start.offset = range.start;
                    end.offset = range.end;
                }
                Anchor::Line(row) => {
                    let pos = live.line_index.get_line_start(*row);
                    *row = self.buffer.line_index.get_line_at(self.project_char(pos));
                }
            }
            projected.add(annotation);
        }
        for cut in &self.cuts {
            projected.add(
                Annotation::new(
                    Kind::new("ghostcut.pending"),
                    Anchor::range(cut.bytes.start, cut.bytes.end),
                    AnnotationOwner::System,
                )
                .with_presentation(
                    Presentation::with_style(StyleOverride {
                        fg: Some(Color::DarkGrey),
                        ..Default::default()
                    })
                    .with_priority(8),
                ),
            );
        }
        self.annotations = projected;
        self.annotation_revision = Some(annotations.revision());
    }
}

impl super::ops::DocumentWrite<'_> {
    pub(crate) fn create_ghosts(&mut self, ranges: &[(usize, usize)], clip: Clip) -> bool {
        self.doc.create_ghosts(ranges, clip)
    }
}

impl Document {
    fn create_ghosts(&mut self, ranges: &[(usize, usize)], clip: Clip) -> bool {
        let mut ranges: Vec<_> = ranges
            .iter()
            .copied()
            .filter(|&(start, end)| start < end)
            .collect();
        if ranges.is_empty() {
            return false;
        }
        ranges.sort_unstable_by_key(|&(start, _)| std::cmp::Reverse(start));
        let buffer = self.buffer.clone();
        self.begin_transaction("Delete");
        let mut cuts = Vec::with_capacity(ranges.len());
        for (start, end) in ranges {
            if self
                .apply_edit(Edit::delete(start..end), EditSource::User)
                .is_ok_and(|changed| changed)
            {
                cuts.push(GhostCut {
                    chars: start..end,
                    bytes: buffer.char_to_byte(start)..buffer.char_to_byte(end),
                    live_char: start,
                    live_byte: 0,
                });
            }
        }
        self.commit_transaction();
        if cuts.is_empty() {
            return false;
        }
        cuts.reverse();
        let mut removed_chars = 0;
        let mut removed_bytes = 0;
        for cut in &mut cuts {
            cut.live_char -= removed_chars;
            cut.live_byte = cut.bytes.start - removed_bytes;
            removed_chars += cut.chars.len();
            removed_bytes += cut.bytes.len();
        }
        let preview = GhostPreview {
            buffer,
            annotations: AnnotationStore::new(),
            cuts,
            clip,
            annotation_revision: None,
        };
        let cursor = preview
            .live_char(preview.buffer.cursor())
            .min(self.buffer.len());
        let _ = self.set_cursor(cursor, CursorCause::Programmatic);
        self.ghost_preview = Some(preview);
        true
    }

    pub(crate) fn most_recent_ghost(&self) -> Option<&Clip> {
        self.ghost_preview.as_ref().map(|preview| &preview.clip)
    }

    pub(crate) fn ghost_at_cursor(&self) -> bool {
        self.ghost_preview
            .as_ref()
            .is_some_and(|preview| preview.cuts[0].live_char == self.buffer.cursor())
    }

    pub(crate) fn restore_ghost(&mut self) -> bool {
        if !self.ghost_at_cursor() || self.write().is_err() {
            return false;
        }
        let Some(preview) = self.ghost_preview.take() else {
            return false;
        };
        let cursor = self.buffer.cursor();
        self.begin_transaction("Put");
        for cut in preview.cuts {
            let text: Vec<_> = preview.buffer.chars(cut.chars.clone()).collect();
            let _ = self.apply_edit(Edit::insert(cut.chars.start, text), EditSource::User);
        }
        self.commit_transaction();
        let _ = self.set_cursor(cursor, CursorCause::Programmatic);
        self.buffer_replaced = true;
        true
    }

    pub(crate) fn commit_pending_ghost(&mut self) {
        self.buffer_replaced |= self.ghost_preview.take().is_some();
    }

    pub(crate) fn ghost_preview(&self) -> Option<&GhostPreview> {
        self.ghost_preview.as_ref()
    }

    pub(crate) fn refresh_ghost_preview(&mut self) {
        if let Some(preview) = &mut self.ghost_preview {
            preview.refresh(&self.buffer, &self.annotations);
        }
    }

    #[cfg(test)]
    pub(crate) fn ghost_count(&self) -> usize {
        self.ghost_preview
            .as_ref()
            .map_or(0, |preview| preview.cuts.len())
    }
}
