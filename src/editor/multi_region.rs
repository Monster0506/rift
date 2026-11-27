use super::Editor;
use crate::character::Character;
use crate::clipboard::Clip;
use crate::document::ops::{Edit, EditSession, EditSource, PutAt};
use crate::document::Document;
use crate::selection::Region;
use crate::term::TerminalBackend;
use crate::wrap::RangeKind;

fn line_start_offset(buf: &crate::buffer::TextBuffer, row: usize) -> usize {
    buf.line_index.get_start(row).unwrap_or(0)
}

fn line_end_offset(buf: &crate::buffer::TextBuffer, row: usize) -> usize {
    if row + 1 < buf.get_total_lines() {
        buf.line_index
            .get_start(row + 1)
            .unwrap_or(buf.len())
            .saturating_sub(1)
    } else {
        buf.len()
    }
}

fn fill_runs(buf: &crate::buffer::TextBuffer, region: &Region) -> Vec<(usize, usize)> {
    use crate::buffer::api::BufferView;

    let (start, end) = region.buffer_span(buf);
    let start = start.min(buf.len());
    let end = end.min(buf.len());

    let mut runs = Vec::new();
    let mut run_start = start;
    for (i, ch) in buf.chars(start..end).enumerate() {
        if ch != Character::Newline {
            continue;
        }
        runs.push((run_start, start + i - run_start));
        run_start = start + i + 1;
    }
    runs.push((run_start, end - run_start));
    runs.retain(|&(_, len)| len > 0);
    runs
}

fn region_clip(buf: &crate::buffer::TextBuffer, region: &Region) -> Clip {
    let range = crate::wrap::MotionRange {
        anchor: region.anchor,
        new_cursor: region.cursor,
        kind: region.kind,
        inclusive: true,
    };
    crate::clipboard::capture_text(buf, &range)
}

fn merge_spans(mut spans: Vec<(usize, usize, bool)>) -> Vec<(usize, usize)> {
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize, bool)> = Vec::with_capacity(spans.len());
    for (start, end, took) in spans {
        match merged.last_mut() {
            Some(last) if start < last.1 => {
                last.1 = last.1.max(end);
                if took && !last.2 {
                    last.0 = last.0.saturating_sub(1);
                    last.2 = true;
                }
            }
            _ => merged.push((start, end, took)),
        }
    }
    merged.into_iter().rev().map(|(s, e, _)| (s, e)).collect()
}

const EXPAND_CANDIDATES: &[(crate::text_objects::ObjectKind, u8)] = &[
    (crate::text_objects::ObjectKind::Word, 1),
    (crate::text_objects::ObjectKind::DoubleQuote, 1),
    (crate::text_objects::ObjectKind::SingleQuote, 1),
    (crate::text_objects::ObjectKind::Backtick, 1),
    (crate::text_objects::ObjectKind::AnyBracket, 1),
    (crate::text_objects::ObjectKind::AnyBracket, 2),
    (crate::text_objects::ObjectKind::AnyBracket, 3),
    (crate::text_objects::ObjectKind::Line, 1),
    (crate::text_objects::ObjectKind::Sentence, 1),
    (crate::text_objects::ObjectKind::Paragraph, 1),
    (crate::text_objects::ObjectKind::Buffer, 1),
];

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn expand_active_region(&mut self) -> bool {
        let Some(anchor) = self.visual_anchor() else {
            return false;
        };
        let Some(doc) = self
            .document_manager
            .get_document(self.active_document_id())
        else {
            return false;
        };
        let cursor = doc.buffer().cursor();
        let current = (anchor.min(cursor), anchor.max(cursor) + 1);

        let mut best: Option<(usize, usize)> = None;
        for &(kind, nesting) in EXPAND_CANDIDATES {
            use crate::text_objects::{Direction, Modifier, TextObjectSpec};
            let spec = TextObjectSpec {
                modifier: Modifier::Around,
                direction: Direction::Current,
                nesting,
                kind,
            };
            let Some(range) = crate::text_objects::resolve(spec, doc.buffer(), 1, None) else {
                continue;
            };
            let end_offset = usize::from(range.inclusive);
            let s = range.anchor.min(range.new_cursor);
            let e = (range.anchor.max(range.new_cursor) + end_offset).min(doc.buffer().len());
            let strictly_larger =
                s <= current.0 && e >= current.1 && (s < current.0 || e > current.1);
            if !strictly_larger {
                continue;
            }
            if best.is_none_or(|(bs, be)| (e - s) < (be - bs)) {
                best = Some((s, e));
            }
        }

        let Some((new_start, new_end)) = best else {
            return false;
        };
        self.expand_history.push(current);
        self.set_visual_anchor(Some(new_start));
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            let _ = doc.set_cursor(
                new_end.saturating_sub(1),
                crate::document::ops::CursorCause::Programmatic,
            );
        }
        true
    }

    pub(super) fn shrink_active_region(&mut self) -> bool {
        if self.visual_anchor().is_none() {
            return false;
        }
        let Some((start, end)) = self.expand_history.pop() else {
            return false;
        };
        self.set_visual_anchor(Some(start));
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            let _ = doc.set_cursor(
                end.saturating_sub(1),
                crate::document::ops::CursorCause::Programmatic,
            );
        }
        true
    }

    pub(super) fn cycle_to_region(&mut self, forward: bool) -> bool {
        let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        else {
            return false;
        };
        if doc.selections().is_empty() {
            self.notify(
                crate::notification::NotificationType::Info,
                "No regions banked".to_string(),
            );
            return false;
        }
        let cursor = doc.buffer().cursor();
        let target = if forward {
            doc.selections().next_region(cursor)
        } else {
            doc.selections().prev_region(cursor)
        };
        let Some(region) = target else { return false };
        let (start, _) = region.span();
        let _ = doc.set_cursor(start, crate::document::ops::CursorCause::User);
        true
    }

    pub(super) fn apply_to_each_region<F>(&mut self, mut f: F) -> bool
    where
        F: FnMut(&mut Self, crate::selection::Region) -> bool,
    {
        self.edit_region_batch(|editor, batch| {
            let mut any = false;
            for region in batch {
                any |= f(editor, region);
            }
            any
        })
    }

    fn edit_region_batch(&mut self, edit: impl FnOnce(&mut Self, Vec<Region>) -> bool) -> bool {
        let doc = self.active_document();
        let batch = doc.take_selection_batch();
        if batch.is_empty() {
            return false;
        }
        let handle = doc.handle();
        doc.start_edit_session(EditSession::MultiRegion);
        let result = edit(self, batch);
        if let Some(doc) = self.document_manager.get_document_by_handle_mut(handle) {
            doc.finish_edit_session();
        }
        result
    }

    pub(super) fn enter_multi_insert<F>(
        &mut self,
        entry: crate::command::Command,
        mut anchor_for: F,
    ) -> bool
    where
        F: FnMut(&mut crate::document::Document, crate::selection::Region) -> usize,
    {
        if !self.can_enter_edit_mode() {
            return false;
        }
        let anchors: Vec<usize> = {
            let Some(doc) = self
                .document_manager
                .get_document_mut(self.active_document_id())
            else {
                return false;
            };
            let batch = doc.take_selection_batch();
            if batch.is_empty() {
                return false;
            }
            doc.start_edit_session(EditSession::MultiInsert);
            let mut anchors: Vec<usize> = Vec::with_capacity(batch.len());
            for region in batch {
                let len_before = doc.buffer().len();
                let new_anchor = anchor_for(doc, region);
                let delta = len_before.cast_signed() - doc.buffer().len().cast_signed();
                if delta != 0 {
                    for a in &mut anchors {
                        *a = ((*a).cast_signed() - delta).cast_unsigned();
                    }
                }
                anchors.push(new_anchor);
            }
            anchors
        };

        let mut anchors = anchors;
        let first = anchors.remove(0);
        self.pending_multi_insert_anchors = anchors;
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            let _ = doc.set_cursor(first, crate::document::ops::CursorCause::Programmatic);
        }
        if !self.dot_repeat.is_replaying() {
            self.dot_repeat.start_insert_recording(entry);
        }
        self.set_mode(crate::mode::Mode::Insert);
        true
    }

    pub(super) fn bank_visual_selection(&mut self) {
        if !self.current_mode.is_visual() {
            return;
        }
        let kind = self.current_mode.visual_range_kind();
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            match kind {
                Some(kind) => doc.bank_selection(kind),
                None => doc.set_selection(None),
            }
        }
    }

    pub(super) fn visual_anchor(&self) -> Option<usize> {
        self.document_manager
            .get_document(self.active_document_id())
            .and_then(Document::visual_anchor)
    }

    pub(super) fn set_visual_anchor(&mut self, anchor: Option<usize>) {
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            doc.set_selection(anchor);
        }
    }

    pub(super) fn try_multi_insert_for_command(&mut self, entry: crate::command::Command) -> bool {
        use crate::command::Command;

        let has_candidate = self.current_mode.is_visual()
            || self
                .document_manager
                .get_document(self.active_document_id())
                .is_some_and(|d| !d.selections().is_empty());
        if !has_candidate {
            return false;
        }
        if !self.can_enter_edit_mode() {
            return true;
        }

        self.bank_visual_selection();

        let is_empty = self
            .document_manager
            .get_document(self.active_document_id())
            .is_none_or(|d| d.selections().is_empty());
        if is_empty {
            return false;
        }

        match entry {
            Command::BeginInsert(crate::command::InsertEntry::AtCursor) => {
                self.enter_multi_insert(entry, |doc, region| region.buffer_span(doc.buffer()).0)
            }
            Command::BeginInsert(crate::command::InsertEntry::AfterCursor) => {
                self.enter_multi_insert(entry, |doc, region| region.buffer_span(doc.buffer()).1)
            }
            Command::BeginInsert(crate::command::InsertEntry::LineStart) => self
                .enter_multi_insert(entry, |doc, region| {
                    let (start, _) = region.buffer_span(doc.buffer());
                    let row = doc.buffer().line_index.get_line_at(start);
                    line_start_offset(doc.buffer(), row)
                }),
            Command::BeginInsert(crate::command::InsertEntry::LineEnd) => {
                self.enter_multi_insert(entry, |doc, region| {
                    let (_, end) = region.buffer_span(doc.buffer());
                    let row = doc.buffer().line_index.get_line_at(end.saturating_sub(1));
                    line_end_offset(doc.buffer(), row)
                })
            }
            Command::BeginInsert(crate::command::InsertEntry::NewLineBelow) => self
                .enter_multi_insert(entry, |doc, region| {
                    let (_, end) = region.buffer_span(doc.buffer());
                    let row = doc.buffer().line_index.get_line_at(end.saturating_sub(1));
                    let target = line_end_offset(doc.buffer(), row);
                    let _ = doc.edit(Edit::insert(target, '\n'), EditSource::User);
                    doc.buffer().cursor()
                }),
            Command::BeginInsert(crate::command::InsertEntry::NewLineAbove) => self
                .enter_multi_insert(entry, |doc, region| {
                    let (start, _) = region.buffer_span(doc.buffer());
                    let row = doc.buffer().line_index.get_line_at(start);
                    let target = line_start_offset(doc.buffer(), row);
                    let _ = doc.edit(Edit::insert(target, '\n'), EditSource::User);
                    target
                }),
            _ => false,
        }
    }

    pub(super) fn try_run_set_aware_operator(&mut self, op: crate::action::OperatorType) -> bool {
        use crate::action::OperatorType;

        let is_empty = self
            .document_manager
            .get_document(self.active_document_id())
            .is_none_or(|d| d.selections().is_empty());
        if is_empty {
            return false;
        }
        if op != OperatorType::Yank && self.active_document().check_edit_access().is_err() {
            return true;
        }

        match op {
            OperatorType::Delete => self.run_set_delete(),
            OperatorType::Yank => {
                let handled = self.apply_to_each_region(|editor, region| {
                    let Some(doc) = editor
                        .document_manager
                        .get_document(editor.active_document_id())
                    else {
                        return false;
                    };
                    editor.yank(region_clip(doc.buffer(), &region));
                    true
                });
                self.refresh_clipboard_buffer_if_open();
                handled
            }
            OperatorType::Change => {
                use crate::command::Command;
                let mut clips = Vec::new();
                let handled = self.enter_multi_insert(
                    Command::BeginInsert(crate::command::InsertEntry::AtCursor),
                    |doc, region| {
                        clips.push(region_clip(doc.buffer(), &region));
                        let (start, end) = region.buffer_span(doc.buffer());
                        let start = start.min(doc.buffer().len());
                        let mut end = end.min(doc.buffer().len());
                        if region.kind == RangeKind::Linewise {
                            let last_line = doc
                                .buffer()
                                .line_index
                                .get_line_at(region.anchor.max(region.cursor));
                            if last_line + 1 < doc.buffer().get_total_lines() {
                                end = end.saturating_sub(1).max(start);
                            }
                        }
                        if start < end {
                            let _ = doc.edit(Edit::delete(start..end), EditSource::User);
                        }
                        start
                    },
                );
                for clip in clips {
                    self.yank(clip);
                }
                self.refresh_clipboard_buffer_if_open();
                handled
            }
        }
    }

    fn run_set_delete(&mut self) -> bool {
        let batch = {
            let Some(doc) = self
                .document_manager
                .get_document_mut(self.active_document_id())
            else {
                return false;
            };
            doc.take_selection_batch()
        };
        if batch.is_empty() {
            return false;
        }
        let (clips, mut ranges): (Vec<Clip>, Vec<(usize, usize)>) = {
            let Some(doc) = self
                .document_manager
                .get_document(self.active_document_id())
            else {
                return false;
            };
            let live: Vec<&crate::selection::Region> = batch
                .iter()
                .filter(|r| {
                    let (s, e) = r.buffer_span(doc.buffer());
                    r.kind == RangeKind::Linewise
                        || s.min(doc.buffer().len()) < e.min(doc.buffer().len())
                })
                .collect();
            let clips = live.iter().map(|r| region_clip(doc.buffer(), r)).collect();
            let spans = live
                .iter()
                .map(|r| {
                    let (start, end) = r.delete_span(doc.buffer());
                    (
                        start.min(doc.buffer().len()),
                        end.min(doc.buffer().len()),
                        start < r.buffer_span(doc.buffer()).0,
                    )
                })
                .collect();
            (clips, merge_spans(spans))
        };
        ranges.retain(|&(start, end)| start < end);
        let ghost_allowed = self
            .document_manager
            .get_document(self.active_document_id())
            .is_some_and(super::super::document::Document::ghost_cut_allowed);
        let ghost_cut = self.state.settings.editing().ghost_cut && ghost_allowed;
        let ghost_clip = clips.last().filter(|_| ghost_cut).cloned();
        for clip in clips {
            self.yank(clip);
        }
        self.refresh_clipboard_buffer_if_open();
        if ranges.is_empty() {
            return true;
        }
        if let Some(ghost_clip) = ghost_clip {
            let cursor_target = ranges.iter().map(|&(s, _)| s).min();
            let ghosted_doc = self
                .document_manager
                .get_document_mut(self.active_document_id())
                .map(|doc| {
                    doc.create_ghosts(&ranges, ghost_clip);
                    if let Some(pos) = cursor_target {
                        let _ =
                            doc.set_cursor(pos, crate::document::ops::CursorCause::Programmatic);
                    }
                    doc.id
                });

            if let Some(id) = ghosted_doc {
                self.set_recent_ghost(Some(id));
            }
            ghosted_doc.is_some()
        } else {
            if let Some(doc) = self
                .document_manager
                .get_document_mut(self.active_document_id())
            {
                doc.edit_group("MultiRegion", |doc| {
                    for &(start, end) in &ranges {
                        let _ = doc.edit(Edit::delete(start..end), EditSource::User);
                    }
                });
            }

            true
        }
    }

    pub(super) fn try_run_set_aware_replace_char(&mut self, ch: char) -> bool {
        let is_empty = self
            .document_manager
            .get_document(self.active_document_id())
            .is_none_or(|d| d.selections().is_empty());
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor
                .document_manager
                .get_document_mut(editor.active_document_id())
            else {
                return false;
            };
            let mut any = false;
            for (pos, len) in fill_runs(doc.buffer(), &region) {
                any |= doc
                    .edit(
                        Edit::replace(pos..pos + len, vec![Character::from(ch); len]),
                        EditSource::User,
                    )
                    .is_ok();
            }
            any
        })
    }

    pub(super) fn try_run_set_aware_delete_surround(&mut self, ch: char, count: usize) -> bool {
        self.apply_to_each_region_surround(ch, count, |editor| {
            editor.execute_buffer_command(crate::command::Command::DeleteSurround(ch, count))
        })
    }

    pub(super) fn try_run_set_aware_change_surround(
        &mut self,
        from: char,
        to: char,
        count: usize,
    ) -> bool {
        self.apply_to_each_region_surround(from, count, |editor| {
            editor.execute_buffer_command(crate::command::Command::ChangeSurround(from, to, count))
        })
    }

    fn apply_to_each_region_surround<F>(
        &mut self,
        resolve_ch: char,
        count: usize,
        mut exec: F,
    ) -> bool
    where
        F: FnMut(&mut Self) -> bool,
    {
        let is_empty = self
            .document_manager
            .get_document(self.active_document_id())
            .is_none_or(|d| d.selections().is_empty());
        if is_empty {
            return false;
        }
        self.edit_region_batch(|editor, batch| {
            let mut any = false;
            let mut consumed: Vec<(usize, usize)> = Vec::new();
            for region in batch {
                if consumed
                    .iter()
                    .any(|&(s, e)| region.anchor >= s && region.anchor < e)
                {
                    continue;
                }
                let Some(doc) = editor
                    .document_manager
                    .get_document_mut(editor.active_document_id())
                else {
                    break;
                };
                let (start, _) = region.buffer_span(doc.buffer());
                let _ = doc.set_cursor(start, crate::document::ops::CursorCause::Programmatic);
                let Some((open_range, close_range)) =
                    crate::text_objects::resolve_surround_pair(resolve_ch, doc.buffer(), count)
                else {
                    continue;
                };
                if exec(editor) {
                    any = true;
                    consumed.push((open_range.start, close_range.end));
                }
            }
            any
        })
    }

    pub(super) fn try_run_set_aware_add_surround(&mut self, ch: char, delim_count: usize) -> bool {
        let is_empty = self
            .document_manager
            .get_document(self.active_document_id())
            .is_none_or(|d| d.selections().is_empty());
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some((open, close)) = crate::text_objects::surround_strings(ch, delim_count) else {
                return false;
            };
            let Some(doc) = editor
                .document_manager
                .get_document_mut(editor.active_document_id())
            else {
                return false;
            };
            let (start, end) = region.buffer_span(doc.buffer());
            let _ = doc.edit(Edit::insert(end, &close), EditSource::User);
            let _ = doc.edit(Edit::insert(start, &open), EditSource::User);
            true
        })
    }

    pub(super) fn try_run_set_aware_put(&mut self, before: bool, clip: &Clip) -> bool {
        let is_empty = self
            .document_manager
            .get_document(self.active_document_id())
            .is_none_or(|d| d.selections().is_empty());
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor
                .document_manager
                .get_document_mut(editor.active_document_id())
            else {
                return false;
            };
            let (start, end) = region.buffer_span(doc.buffer());
            doc.put(clip, PutAt::Region { start, end, before })
        })
    }

    pub(super) fn replay_multi_insert_at_remaining_anchors(&mut self) {
        let anchors = std::mem::take(&mut self.pending_multi_insert_anchors);
        let Some(crate::dot_repeat::DotRegister::InsertSession { commands, .. }) =
            self.dot_repeat.register().cloned()
        else {
            return;
        };
        let was_replaying = self.dot_repeat.is_replaying();
        self.dot_repeat.set_replaying(true);
        for anchor in anchors {
            if let Some(doc) = self
                .document_manager
                .get_document_mut(self.active_document_id())
            {
                let _ = doc.set_cursor(anchor, crate::document::ops::CursorCause::Programmatic);
            }
            for &cmd in &commands {
                self.execute_buffer_command(cmd);
            }
        }
        self.dot_repeat.set_replaying(was_replaying);
    }

    pub(super) fn finish_region_build(&mut self, follow_up: Option<crate::action::Action>) {
        if self.region_build_recording.is_empty() {
            return;
        }
        let actions = std::mem::take(&mut self.region_build_recording);
        if !self.dot_repeat.is_replaying() {
            self.dot_repeat
                .record_region_build_session(actions, follow_up);
        }
    }

    pub(super) fn toggle_regions_window(&mut self) {
        if let Some(layout) = self.panel_layout.clone() {
            if layout.kind == crate::editor::PanelKind::Regions {
                self.close_split_panel();
                return;
            }
        }

        let Some(source_doc_id) = self
            .document_manager
            .get_document(self.active_document_id())
            .map(|d| d.id)
        else {
            return;
        };
        let regions = self
            .document_manager
            .get_document(self.active_document_id())
            .map_or_default(|d| d.selections().sorted());
        if regions.is_empty() {
            self.notify(
                crate::notification::NotificationType::Info,
                "No regions banked".to_string(),
            );
            return;
        }

        let list_doc_id = self.document_manager.next_id();
        let mut doc = match crate::document::Document::new(list_doc_id) {
            Ok(d) => d,
            Err(e) => {
                self.notify_error(e);
                return;
            }
        };
        if let Some(source) = self
            .document_manager
            .get_document(self.active_document_id())
        {
            doc.populate_regions_buffer(source.buffer(), &regions);
        }
        doc.set_regions(source_doc_id);
        if self.open_split_panel(
            crate::editor::PanelKind::Regions,
            doc,
            None,
            super::ops::windows::PanelSplit::Below,
        ) {
            let _ = self.force_full_redraw();
        }
    }

    pub(super) fn drop_regions_window_entry(&mut self) -> bool {
        let Some(layout) = self.panel_layout.clone() else {
            return false;
        };
        if layout.kind != crate::editor::PanelKind::Regions {
            return false;
        }
        let line = self
            .document_manager
            .get_document(self.active_document_id())
            .map_or(0, |d| {
                d.buffer().line_index.get_line_at(d.buffer().cursor())
            });
        let Some(source_doc_id) = self
            .document_manager
            .get_document(self.active_document_id())
            .and_then(super::super::document::Document::regions_source_doc_id)
        else {
            return false;
        };
        let Some(source) = self.document_manager.get_document_mut(source_doc_id) else {
            return false;
        };
        let sorted = source.selections().sorted();
        let Some(target) = sorted.get(line).copied() else {
            return false;
        };
        source.unbank_region(target);
        let remaining = source.selections().sorted();
        let source_buf = source.buffer().clone();
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            doc.populate_regions_buffer(&source_buf, &remaining);
        }
        true
    }
}
