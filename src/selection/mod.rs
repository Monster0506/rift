//! Multi-region, non-contiguous selection set (visual-mode-design.md).
//!
//! Regions are plain char-offset anchor/cursor pairs, not edit-tracked markers --
//! any edit outside the set-aware drivers clears the set (Document::undo/redo/goto_seq).

use crate::buffer::TextBuffer;
use crate::wrap::RangeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub anchor: usize,
    pub cursor: usize,
    pub kind: RangeKind,
}

impl Region {
    pub fn new(anchor: usize, cursor: usize, kind: RangeKind) -> Self {
        Self { anchor, cursor, kind }
    }

    /// Ordered (start, end) char-offset span, `end` exclusive; pure anchor/cursor
    /// math for set bookkeeping (see `buffer_span` for the buffer-aware range).
    pub fn span(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor) + 1)
    }

    /// Range this region covers in `buf`: same as `span()` except Linewise is
    /// expanded to whole lines. Use this, not `span()`, to read/mutate buffer text.
    pub fn buffer_span(&self, buf: &TextBuffer) -> (usize, usize) {
        match self.kind {
            RangeKind::Linewise => {
                let first = self.anchor.min(self.cursor);
                let last = self.anchor.max(self.cursor);
                let first_line = buf.line_index.get_line_at(first);
                let last_line = buf.line_index.get_line_at(last);
                let s = buf.line_index.get_start(first_line).unwrap_or(0);
                let e = if last_line + 1 < buf.get_total_lines() {
                    buf.line_index.get_start(last_line + 1).unwrap_or(buf.len())
                } else {
                    buf.len()
                };
                (s, e)
            }
            RangeKind::Charwise | RangeKind::Blockwise => self.span(),
        }
    }

    fn overlaps(&self, other: &Region) -> bool {
        if self.kind != other.kind {
            return false;
        }
        let (a_start, a_end) = self.span();
        let (b_start, b_end) = other.span();
        a_start < b_end && b_start < a_end
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelectionSet {
    pub regions: Vec<Region>,
    pub active: Option<Region>,
}

impl SelectionSet {
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty() && self.active.is_none()
    }

    pub fn clear(&mut self) {
        self.regions.clear();
        self.active = None;
    }

    /// Merge `region` into the set, repeating while it overlaps another
    /// same-kind region (touching does not count -- see design doc S3).
    pub fn bank(&mut self, region: Region) {
        let mut cur = region;
        loop {
            let Some(idx) = self.regions.iter().position(|r| r.overlaps(&cur)) else {
                break;
            };
            let other = self.regions.remove(idx);
            let (a_start, a_end) = cur.span();
            let (b_start, b_end) = other.span();
            let start = a_start.min(b_start);
            let end = a_end.max(b_end);
            cur = Region::new(start, end.saturating_sub(1), cur.kind);
        }
        self.regions.push(cur);
    }

    pub fn commit_active(&mut self) {
        if let Some(region) = self.active.take() {
            self.bank(region);
        }
    }

    pub fn sorted(&self) -> Vec<Region> {
        let mut v = self.regions.clone();
        v.sort_by_key(|r| r.span().0);
        v
    }

    pub fn take_for_batch(&mut self) -> Vec<Region> {
        self.commit_active();
        let mut v = std::mem::take(&mut self.regions);
        v.sort_by_key(|r| std::cmp::Reverse(r.span().0));
        v
    }

    pub fn region_containing(&self, offset: usize) -> Option<usize> {
        self.regions.iter().position(|r| {
            let (s, e) = r.span();
            s <= offset && offset < e
        })
    }

    pub fn next_region(&self, after: usize) -> Option<Region> {
        let sorted = self.sorted();
        if sorted.is_empty() {
            return None;
        }
        sorted
            .iter()
            .find(|r| r.span().0 > after)
            .or(sorted.first())
            .copied()
    }

    pub fn prev_region(&self, before: usize) -> Option<Region> {
        let sorted = self.sorted();
        if sorted.is_empty() {
            return None;
        }
        sorted
            .iter()
            .rev()
            .find(|r| r.span().1 <= before)
            .or(sorted.last())
            .copied()
    }

    /// `m`/`M`: bank the next/previous literal occurrence of the most recently
    /// banked region's text via `crate::search::find_next`. Disabled for Blockwise (S7).
    pub fn bank_occurrence(&mut self, buf: &TextBuffer, forward: bool) -> Option<(Region, String)> {
        use crate::buffer::api::BufferView;
        use crate::search::{find_next, SearchDirection};

        let last = *self.regions.last()?;
        if last.kind == RangeKind::Blockwise {
            return None;
        }
        let (last_start, last_end) = last.span();
        let needle: String = buf.chars(last_start..last_end).map(|c| c.to_string()).collect();
        if needle.is_empty() {
            return None;
        }
        let (direction, start_pos) = if forward {
            (SearchDirection::Forward, last_end)
        } else {
            (SearchDirection::Backward, last_start)
        };
        let (m, _stats) = find_next(buf, start_pos, &needle, direction).ok()?;
        let m = m?;
        let new_start = m.range.start;
        let new_end = m.range.end.saturating_sub(1);
        let already_contained = self.regions.iter().any(|r| {
            let (s, e) = r.span();
            s <= new_start && new_end < e
        });
        if already_contained {
            return None;
        }
        let region = Region::new(new_start, new_end, last.kind);
        self.bank(region);
        Some((region, needle))
    }
}

#[cfg(test)]
mod tests;
