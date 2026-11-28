use super::super::Document;
use super::{CursorCause, DocumentWrite, Edit, EditSource};
use crate::action::Motion;
use crate::buffer::api::BufferView;
use crate::character::Character;
use crate::clipboard::Clip;
use crate::selection::{Region, SelectionSet, Target, TargetSet};
use crate::wrap::RangeKind;

#[derive(Clone, Copy)]
pub(crate) enum DeleteLanding {
    AtEdit,
    CountedRows,
    FirstNonblank,
}

fn deletion_spans(targets: &[Target]) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::with_capacity(targets.len());
    let mut terminal_rows = false;
    for target in targets {
        let start = target.delete_start();
        if start == target.end {
            continue;
        }
        if let Some((next_start, next_end)) = spans.last_mut() {
            if target.end > *next_start {
                terminal_rows &= target.kind == RangeKind::Linewise;
                *next_start = (*next_start).min(start.saturating_sub(usize::from(terminal_rows)));
                *next_end = (*next_end).max(target.end);
                continue;
            }
        }
        spans.push((start, target.end));
        terminal_rows = target.last_line;
    }
    spans
}

impl DocumentWrite<'_> {
    pub(crate) fn clear_target_selection(&mut self) {
        self.doc.clear_target_selection();
    }

    pub(crate) fn motion_targets(
        &mut self,
        motion: Motion,
        count: usize,
        viewport_height: usize,
        query: Option<&str>,
    ) -> Option<TargetSet> {
        self.doc
            .motion_targets(motion, count, viewport_height, query)
    }

    pub(crate) fn delete_targets(
        &mut self,
        targets: &TargetSet,
        landing: DeleteLanding,
        ghost: Option<&Clip>,
    ) -> (bool, bool) {
        let single;
        let batch;
        let spans = match targets {
            TargetSet::One(target) => {
                let start = target.delete_start();
                if start == target.end {
                    return (false, false);
                }
                single = [(start, target.end)];
                &single[..]
            }
            TargetSet::Many(targets) => {
                batch = deletion_spans(targets);
                &batch[..]
            }
        };
        let use_ghost = ghost.filter(|_| self.ghost_cut_allowed());
        let Some(&(first, _)) = spans.last() else {
            return (false, false);
        };
        let (changed, ghosted) = if let Some(clip) = use_ghost {
            let ghosted = self.create_ghosts(spans, clip.clone());
            if ghosted {
                let _ = self.set_cursor(first, CursorCause::Programmatic);
            }
            (ghosted, ghosted)
        } else {
            let label = match (targets, landing, self.is_clipboard_index()) {
                (_, DeleteLanding::CountedRows, true) => "Delete clipboard rows",
                (_, DeleteLanding::CountedRows, false) => "DeleteLine",
                (TargetSet::Many(_), _, _) => "MultiRegion",
                _ => "Delete",
            };
            let changed = self.edit_group(label, |doc| {
                let mut changed = false;
                for &(start, end) in spans {
                    changed |= doc
                        .edit(Edit::delete(start..end), EditSource::User)
                        .is_ok_and(|applied| applied);
                }
                if changed
                    && matches!(landing, DeleteLanding::CountedRows)
                    && !doc.is_clipboard_index()
                    && matches!(targets, TargetSet::One(target) if target.last_line)
                {
                    let start = doc.buffer().line_start(doc.buffer().get_line());
                    let _ = doc.set_cursor(start, CursorCause::Programmatic);
                }
                changed
            });
            (changed, false)
        };
        if changed && matches!(landing, DeleteLanding::FirstNonblank) {
            let buf = self.buffer();
            let start = buf.line_start(buf.get_line());
            let column = buf
                .chars(start..buf.len())
                .take_while(|&ch| ch != Character::Newline)
                .position(|ch| !matches!(ch, Character::Unicode(' ') | Character::Tab))
                .unwrap_or(0);
            let _ = self.set_cursor(start + column, CursorCause::Programmatic);
        }
        (changed, ghosted)
    }
}

impl Document {
    pub(crate) const fn selections(&self) -> &SelectionSet {
        &self.selection_set
    }

    pub(crate) const fn visual_anchor(&self) -> Option<usize> {
        self.visual_anchor
    }

    pub(crate) fn set_selection(&mut self, anchor: Option<usize>) {
        if self.visual_anchor != anchor {
            self.visual_anchor = anchor;
            self.record_selection_change();
        }
    }

    pub(crate) fn clear_selection(&mut self) {
        if !self.selection_set.is_empty() {
            self.selection_set.clear();
            self.record_selection_change();
        }
    }

    pub(crate) fn bank_region(&mut self, region: Region) {
        self.selection_set.bank(region, &self.buffer);
        self.record_selection_change();
    }

    pub(crate) fn bank_selection(&mut self, kind: RangeKind) {
        if let Some(anchor) = self.visual_anchor.take() {
            let region = Region::new(anchor, self.buffer.cursor(), kind);
            self.bank_region(region);
        }
    }

    pub(crate) fn make_banked_regions_linewise(&mut self) {
        if !self.selection_set.is_empty() {
            self.selection_set.make_linewise(&self.buffer);
            self.record_selection_change();
        }
    }

    pub(crate) fn bank_occurrence(&mut self, forward: bool) -> Option<(Region, String)> {
        let banked = self.selection_set.bank_occurrence(&self.buffer, forward);
        if banked.is_some() {
            self.record_selection_change();
        }
        banked
    }

    pub(crate) fn take_selection_batch(&mut self) -> Vec<Region> {
        let batch = self.selection_set.take_for_batch(&self.buffer);
        if !batch.is_empty() {
            self.record_selection_change();
        }
        batch
    }

    pub(crate) fn motion_targets(
        &mut self,
        motion: Motion,
        count: usize,
        viewport_height: usize,
        query: Option<&str>,
    ) -> Option<TargetSet> {
        let range =
            super::cursor::compute_motion_range(motion, count, self, viewport_height, query)?;
        TargetSet::from_motion(&range, &self.buffer)
    }

    pub(crate) fn selection_targets(&self, visual: Option<RangeKind>) -> Option<TargetSet> {
        let region = visual.and_then(|kind| self.visual_region(kind));
        if self.selection_set.is_empty() {
            return Some(TargetSet::one(region?, &self.buffer));
        }
        let mut selections = self.selection_set.clone();
        if let Some(region) = region {
            selections.bank(region, &self.buffer);
        }
        let batch = selections.take_for_batch(&self.buffer);
        TargetSet::from_batch(batch, &self.buffer)
    }

    pub(crate) fn clear_target_selection(&mut self) {
        if self.visual_anchor.take().is_some() || !self.selection_set.is_empty() {
            self.selection_set.clear();
            self.record_selection_change();
        }
    }

    pub(crate) fn visual_region(&self, kind: RangeKind) -> Option<Region> {
        Some(Region::new(self.visual_anchor?, self.buffer.cursor(), kind))
    }

    pub(crate) fn resume_selection(&mut self, kind: RangeKind) {
        let cursor = self.buffer.cursor();
        let banked = self
            .selection_set
            .region_containing(cursor)
            .filter(|&idx| self.selection_set.regions[idx].kind == kind);
        match banked {
            Some(idx) => {
                let region = self.selection_set.regions.remove(idx);
                self.visual_anchor = Some(region.anchor);
                let _ = self.set_cursor(region.cursor, super::CursorCause::Programmatic);
            }
            None => self.visual_anchor = Some(cursor),
        }
        self.record_selection_change();
    }

    pub(crate) fn unbank_region(&mut self, region: Region) {
        let before = self.selection_set.regions.len();
        self.selection_set.regions.retain(|r| *r != region);
        if self.selection_set.regions.len() != before {
            self.record_selection_change();
        }
    }
}
