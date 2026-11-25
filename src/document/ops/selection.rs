use super::super::Document;
use crate::selection::{Region, SelectionSet};
use crate::wrap::RangeKind;

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
                let _ = self.set_cursor(region.cursor, super::CursorCause::User);
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
