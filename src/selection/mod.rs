//! Multi-region, non-contiguous selection set. Regions are plain char-offset
//! pairs; any edit outside the set-aware drivers clears the set.

use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::wrap::RangeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Region {
    pub(crate) anchor: usize,
    pub(crate) cursor: usize,
    pub(crate) kind: RangeKind,
}

pub(crate) enum TargetSet {
    One(Target),
    Many(Vec<Target>),
}

impl TargetSet {
    pub(crate) fn one(region: Region, buf: &TextBuffer) -> Self {
        Self::One(region.resolve(buf))
    }

    pub(crate) fn from_motion(range: &crate::wrap::MotionRange, buf: &TextBuffer) -> Option<Self> {
        Some(Self::one(Region::from_motion(range)?, buf))
    }

    pub(crate) fn current_lines(buf: &TextBuffer, count: usize) -> Option<Self> {
        let line = buf.get_line();
        let last_real = buf.last_real_line();
        if line > last_real {
            return None;
        }
        let last = line.saturating_add(count.max(1) - 1).min(last_real);
        let region = Region::new(
            buf.line_start(line),
            buf.line_start(last),
            RangeKind::Linewise,
        );
        Some(Self::one(region, buf))
    }

    pub(crate) fn from_batch(regions: Vec<Region>, buf: &TextBuffer) -> Option<Self> {
        let mut targets: Vec<_> = regions
            .into_iter()
            .map(|region| region.resolve(buf))
            .collect();
        targets.dedup_by(|left, right| {
            if left.end <= right.start {
                return false;
            }
            right.start = left.start.min(right.start);
            right.end = left.end.max(right.end);
            true
        });
        match targets.as_slice() {
            [] => None,
            [target] => Some(Self::One(*target)),
            _ => Some(Self::Many(targets)),
        }
    }

    pub(crate) fn targets(&self) -> &[Target] {
        match self {
            Self::One(target) => std::slice::from_ref(target),
            Self::Many(targets) => targets,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Target {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) kind: RangeKind,
    pub(crate) last_line: bool,
}

impl Target {
    pub(crate) fn delete_start(&self) -> usize {
        self.start.saturating_sub(usize::from(self.last_line))
    }

    pub(crate) fn whole_rows(&self, buf: &TextBuffer) -> Option<std::ops::RangeInclusive<usize>> {
        let mut first = buf.line_index.get_line_at(self.start);
        if self.kind == RangeKind::Linewise {
            let last = buf.line_index.get_line_at(if self.last_line {
                self.end
            } else {
                self.end.saturating_sub(1)
            });
            return Some(first..=last);
        }
        if self.start == self.end {
            return None;
        }
        first += usize::from(self.start != buf.line_start(first));
        let last = buf.line_index.get_line_at(self.end - 1);
        let end = buf.line_index.get_end(last, buf.len())?;
        let last = last.checked_sub(usize::from(self.end < end))?;
        (first <= last).then_some(first..=last)
    }
}

impl Region {
    #[must_use]
    pub(crate) const fn new(anchor: usize, cursor: usize, kind: RangeKind) -> Self {
        Self {
            anchor,
            cursor,
            kind,
        }
    }

    pub(crate) fn from_motion(range: &crate::wrap::MotionRange) -> Option<Self> {
        let first = range.anchor.min(range.new_cursor);
        let last = range.anchor.max(range.new_cursor);
        let last = if range.kind != RangeKind::Linewise && !range.inclusive {
            last.checked_sub(1).filter(|last| *last >= first)?
        } else {
            last
        };
        Some(Self::new(first, last, range.kind))
    }

    pub(crate) fn resolve(&self, buf: &TextBuffer) -> Target {
        if self.kind == RangeKind::Linewise {
            let index = &buf.line_index;
            let first = index.get_line_at(self.anchor.min(self.cursor));
            let last = index.get_line_at(self.anchor.max(self.cursor));
            let next = index.get_start(last + 1);
            return Target {
                start: buf.line_start(first),
                end: next.unwrap_or(buf.len()),
                kind: self.kind,
                last_line: next.is_none(),
            };
        }
        let (start, end) = self.span();
        Target {
            start: start.min(buf.len()),
            end: end.min(buf.len()),
            kind: self.kind,
            last_line: false,
        }
    }

    /// Ordered exclusive-end span for selection bookkeeping.
    #[must_use]
    pub(crate) fn span(&self) -> (usize, usize) {
        (
            self.anchor.min(self.cursor),
            self.anchor.max(self.cursor) + 1,
        )
    }

    /// Return this region's buffer range, expanding linewise selections to whole lines.
    pub(crate) fn buffer_span(&self, buf: &TextBuffer) -> (usize, usize) {
        match self.kind {
            RangeKind::Linewise => {
                let target = self.resolve(buf);
                (target.start, target.end)
            }
            RangeKind::Charwise | RangeKind::Blockwise => self.span(),
        }
    }

    #[cfg(test)]
    pub(crate) fn delete_span(&self, buf: &TextBuffer) -> (usize, usize) {
        let target = self.resolve(buf);
        (target.delete_start(), target.end)
    }

    fn overlaps(&self, other: &Self, buf: &TextBuffer) -> bool {
        let has_linewise = self.kind == RangeKind::Linewise || other.kind == RangeKind::Linewise;
        if self.kind != other.kind && !has_linewise {
            return false;
        }
        let (a_start, a_end) = self.buffer_span(buf);
        let (b_start, b_end) = other.buffer_span(buf);
        a_start < b_end && b_start < a_end
    }

    #[cfg(any(debug_assertions, test))]
    fn check_invariants(&self, buf: &TextBuffer) -> Vec<String> {
        let mut out = Vec::new();
        if self.anchor == usize::MAX || self.cursor == usize::MAX {
            out.push(format!(
                "region anchor/cursor at usize::MAX (span() would overflow): anchor={} cursor={}",
                self.anchor, self.cursor
            ));
        }
        let len = buf.len();
        if self.anchor > len {
            out.push(format!("region anchor {} > buffer len {len}", self.anchor));
        }
        if self.cursor > len {
            out.push(format!("region cursor {} > buffer len {len}", self.cursor));
        }
        out
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SelectionSet {
    pub(crate) regions: Vec<Region>,
    active: Option<Region>,
}

impl SelectionSet {
    #[must_use]
    pub(crate) const fn is_empty(&self) -> bool {
        self.regions.is_empty() && self.active.is_none()
    }

    pub(crate) fn clear(&mut self) {
        self.regions.clear();
        self.active = None;
    }

    /// Merge overlapping regions; touching regions stay separate. A linewise
    /// region absorbs overlapping regions of other kinds. Charwise and
    /// blockwise regions never merge with each other.
    pub(crate) fn bank(&mut self, region: Region, buf: &TextBuffer) {
        let mut cur = region;
        while let Some(idx) = self.regions.iter().position(|r| r.overlaps(&cur, buf)) {
            let other = self.regions.remove(idx);
            let (a_start, a_end) = cur.buffer_span(buf);
            let (b_start, b_end) = other.buffer_span(buf);
            let kind = if cur.kind == other.kind {
                cur.kind
            } else {
                RangeKind::Linewise
            };
            let start = a_start.min(b_start);
            let end = a_end.max(b_end);
            cur = Region::new(start, end.saturating_sub(1), kind);
        }
        self.regions.push(cur);
    }

    pub(super) fn make_linewise(&mut self, buf: &TextBuffer) {
        self.commit_active(buf);
        for region in std::mem::take(&mut self.regions) {
            self.bank(
                Region {
                    kind: RangeKind::Linewise,
                    ..region
                },
                buf,
            );
        }
    }

    fn commit_active(&mut self, buf: &TextBuffer) {
        if let Some(region) = self.active.take() {
            self.bank(region, buf);
        }
    }

    #[must_use]
    pub(crate) fn sorted(&self) -> Vec<Region> {
        let mut v = self.regions.clone();
        v.sort_by_key(|r| r.span().0);
        v
    }

    pub(crate) fn take_for_batch(&mut self, buf: &TextBuffer) -> Vec<Region> {
        self.commit_active(buf);
        let mut v = std::mem::take(&mut self.regions);
        v.sort_by_key(|r| std::cmp::Reverse(r.span().0));
        let mut seen = std::collections::HashSet::new();
        v.retain(|r| seen.insert(r.buffer_span(buf)));
        v
    }

    #[must_use]
    pub(crate) fn region_containing(&self, offset: usize) -> Option<usize> {
        self.regions.iter().position(|r| {
            let (s, e) = r.span();
            s <= offset && offset < e
        })
    }

    #[must_use]
    pub(crate) fn next_region(&self, after: usize) -> Option<Region> {
        let sorted = self.sorted();
        if sorted.is_empty() {
            return None;
        }
        sorted
            .iter()
            .find(|r| r.span().0 > after)
            .or_else(|| sorted.first())
            .copied()
    }

    #[must_use]
    pub(crate) fn prev_region(&self, before: usize) -> Option<Region> {
        let sorted = self.sorted();
        if sorted.is_empty() {
            return None;
        }
        sorted
            .iter()
            .rev()
            .find(|r| r.span().1 <= before)
            .or_else(|| sorted.last())
            .copied()
    }

    /// `m`/`M`: bank the next/previous literal occurrence of the most recently
    /// banked region's text via `crate::search::find_next`. Disabled for Blockwise (S7).
    pub(crate) fn bank_occurrence(
        &mut self,
        buf: &TextBuffer,
        forward: bool,
    ) -> Option<(Region, String)> {
        use crate::buffer::api::BufferView;
        use crate::search::{find_next, SearchDirection};

        let last = *self.regions.last()?;
        if last.kind == RangeKind::Blockwise {
            return None;
        }
        // buffer_span, not span: Linewise must match the whole line's text, not the raw
        // anchor/cursor range (which collapses to one char when anchor == cursor).
        let (last_start, last_end) = last.buffer_span(buf);
        let needle: String = buf
            .chars(last_start..last_end)
            .map(|c| c.to_string())
            .collect();
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
            let (s, e) = r.buffer_span(buf);
            s <= new_start && new_end < e
        });
        if already_contained {
            return None;
        }
        let region = Region::new(new_start, new_end, last.kind);
        self.bank(region, buf);
        Some((region, needle))
    }

    #[cfg(any(debug_assertions, test))]
    pub(crate) fn check_invariants(
        &self,
        buf: &TextBuffer,
        tier: crate::invariants::InvariantTier,
    ) -> Vec<String> {
        use crate::invariants::InvariantTier;

        let mut out = Vec::new();
        for region in self.regions.iter().chain(self.active.iter()) {
            out.extend(region.check_invariants(buf));
        }

        if tier >= InvariantTier::Standard {
            for i in 0..self.regions.len() {
                for j in (i + 1)..self.regions.len() {
                    if self.regions[i].overlaps(&self.regions[j], buf) {
                        out.push(format!(
                            "regions {i} and {j} overlap: {:?} and {:?}",
                            self.regions[i], self.regions[j]
                        ));
                    }
                }
            }

            let line_count = buf.get_total_lines();
            for (i, region) in self.regions.iter().enumerate() {
                let anchor_line = buf.line_index.get_line_at(region.anchor.min(buf.len()));
                let cursor_line = buf.line_index.get_line_at(region.cursor.min(buf.len()));
                if anchor_line >= line_count || cursor_line >= line_count {
                    out.push(format!(
                        "region {i} references line outside buffer: anchor_line={anchor_line} cursor_line={cursor_line} line_count={line_count}"
                    ));
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests;
