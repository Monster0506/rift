use super::Editor;
use crate::character::Character;
use crate::term::TerminalBackend;

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
        let Some(anchor) = self.visual_anchor else {
            return false;
        };
        let Some(doc) = self.document_manager.active_document() else {
            return false;
        };
        let cursor = doc.buffer.cursor();
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
            let Some(range) = crate::text_objects::resolve(spec, &doc.buffer, 1, None) else {
                continue;
            };
            let end_offset = if range.inclusive { 1 } else { 0 };
            let s = range.anchor.min(range.new_cursor);
            let e = (range.anchor.max(range.new_cursor) + end_offset).min(doc.buffer.len());
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
        self.visual_anchor = Some(new_start);
        if let Some(doc) = self.document_manager.active_document_mut() {
            let _ = doc.buffer.set_cursor(new_end.saturating_sub(1));
        }
        true
    }

    pub(super) fn shrink_active_region(&mut self) -> bool {
        if self.visual_anchor.is_none() {
            return false;
        }
        let Some((start, end)) = self.expand_history.pop() else {
            return false;
        };
        self.visual_anchor = Some(start);
        if let Some(doc) = self.document_manager.active_document_mut() {
            let _ = doc.buffer.set_cursor(end.saturating_sub(1));
        }
        true
    }

    pub(super) fn cycle_to_region(&mut self, forward: bool) -> bool {
        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        if doc.selection_set.is_empty() {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "No regions banked".to_string(),
            );
            return false;
        }
        let cursor = doc.buffer.cursor();
        let target = if forward {
            doc.selection_set.next_region(cursor)
        } else {
            doc.selection_set.prev_region(cursor)
        };
        let Some(region) = target else { return false };
        let (start, _) = region.span();
        let _ = doc.buffer.set_cursor(start);
        true
    }

    pub(super) fn apply_to_each_region<F>(&mut self, mut f: F) -> bool
    where
        F: FnMut(&mut Self, crate::selection::Region) -> bool,
    {
        let batch = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return false;
            };
            doc.selection_set.take_for_batch(&doc.buffer)
        };
        if batch.is_empty() {
            return false;
        }
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.begin_transaction("MultiRegion");
        }
        let mut any = false;
        for region in batch {
            if f(self, region) {
                any = true;
            }
        }
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.commit_transaction();
        }
        self.do_incremental_syntax_parse();
        any
    }

    pub(super) fn enter_multi_insert<F>(
        &mut self,
        entry: crate::command::Command,
        mut anchor_for: F,
    ) -> bool
    where
        F: FnMut(&mut crate::document::Document, crate::selection::Region) -> usize,
    {
        let anchors: Vec<usize> = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return false;
            };
            let batch = doc.selection_set.take_for_batch(&doc.buffer);
            if batch.is_empty() {
                return false;
            }
            doc.begin_transaction("MultiInsert");
            let mut anchors: Vec<usize> = Vec::with_capacity(batch.len());
            for region in batch {
                let len_before = doc.buffer.len();
                let new_anchor = anchor_for(doc, region);
                let delta = len_before as i64 - doc.buffer.len() as i64;
                if delta != 0 {
                    for a in anchors.iter_mut() {
                        *a = (*a as i64 - delta) as usize;
                    }
                }
                anchors.push(new_anchor);
            }
            anchors
        };
        self.do_incremental_syntax_parse();
        let mut anchors = anchors;
        let first = anchors.remove(0);
        self.pending_multi_insert_anchors = anchors;
        if let Some(doc) = self.document_manager.active_document_mut() {
            let _ = doc.buffer.set_cursor(first);
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
        if let (Some(anchor), Some(kind)) =
            (self.visual_anchor, self.current_mode.visual_range_kind())
        {
            if let Some(doc) = self.document_manager.active_document_mut() {
                let cursor = doc.buffer.cursor();
                doc.selection_set
                    .bank(crate::selection::Region::new(anchor, cursor, kind));
            }
        }
        self.visual_anchor = None;
    }

    pub(super) fn try_multi_insert_for_command(&mut self, entry: crate::command::Command) -> bool {
        use crate::command::Command;

        let has_candidate = self.current_mode.is_visual()
            || self
                .document_manager
                .active_document()
                .is_some_and(|d| !d.selection_set.is_empty());
        if !has_candidate || self.active_doc_is(|d| d.is_read_only()) {
            return false;
        }

        self.bank_visual_selection();

        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }

        match entry {
            Command::EnterInsertMode => {
                self.enter_multi_insert(entry, |doc, region| region.buffer_span(&doc.buffer).0)
            }
            Command::EnterInsertModeAfter => {
                self.enter_multi_insert(entry, |doc, region| region.buffer_span(&doc.buffer).1)
            }
            Command::EnterInsertModeAtLineStart => self.enter_multi_insert(entry, |doc, region| {
                let (start, _) = region.buffer_span(&doc.buffer);
                let row = doc.buffer.line_index.get_line_at(start);
                line_start_offset(&doc.buffer, row)
            }),
            Command::EnterInsertModeAtLineEnd => self.enter_multi_insert(entry, |doc, region| {
                let (_, end) = region.buffer_span(&doc.buffer);
                let row = doc.buffer.line_index.get_line_at(end.saturating_sub(1));
                line_end_offset(&doc.buffer, row)
            }),
            Command::OpenLineBelow => self.enter_multi_insert(entry, |doc, region| {
                let (_, end) = region.buffer_span(&doc.buffer);
                let row = doc.buffer.line_index.get_line_at(end.saturating_sub(1));
                let target = line_end_offset(&doc.buffer, row);
                let _ = doc.buffer.set_cursor(target);
                let _ = doc.insert_char('\n');
                doc.buffer.cursor()
            }),
            Command::OpenLineAbove => self.enter_multi_insert(entry, |doc, region| {
                let (start, _) = region.buffer_span(&doc.buffer);
                let row = doc.buffer.line_index.get_line_at(start);
                let target = line_start_offset(&doc.buffer, row);
                let _ = doc.buffer.set_cursor(target);
                let _ = doc.insert_char('\n');
                target
            }),
            _ => false,
        }
    }

    pub(super) fn try_run_set_aware_operator(&mut self, op: crate::action::OperatorType) -> bool {
        use crate::action::OperatorType;
        use crate::buffer::api::BufferView;

        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }

        match op {
            OperatorType::Delete => {
                let batch = {
                    let Some(doc) = self.document_manager.active_document_mut() else {
                        return false;
                    };
                    doc.selection_set.take_for_batch(&doc.buffer)
                };
                if batch.is_empty() {
                    return false;
                }
                let committed_prior = self
                    .document_manager
                    .active_document_mut()
                    .is_some_and(|doc| doc.commit_pending_ghost());
                if committed_prior {
                    self.do_incremental_syntax_parse();
                }
                let ranges: Vec<(usize, usize)> = {
                    let Some(doc) = self.document_manager.active_document() else {
                        return false;
                    };
                    batch
                        .iter()
                        .map(|region| region.buffer_span(&doc.buffer))
                        .filter(|&(s, e)| s < e)
                        .collect()
                };
                if ranges.is_empty() {
                    return false;
                }
                for &(start, end) in &ranges {
                    let Some(doc) = self.document_manager.active_document() else {
                        continue;
                    };
                    let text: Vec<Character> = doc.buffer.chars(start..end).collect();
                    self.clipboard_ring.push(text);
                }
                self.refresh_clipboard_buffer_if_open();
                let ghost_allowed = self
                    .document_manager
                    .active_document()
                    .is_some_and(|doc| doc.ghost_cut_allowed());
                if self.state.settings.ghost_cut && ghost_allowed {
                    let cursor_target = ranges.iter().map(|&(s, _)| s).min();
                    let ghosted_doc = self.document_manager.active_document_mut().map(|doc| {
                        doc.create_ghosts(&ranges);
                        if let Some(pos) = cursor_target {
                            let _ = doc.buffer.set_cursor(pos);
                        }
                        doc.id
                    });
                    if let Some(id) = ghosted_doc {
                        self.document_manager.set_most_recent_ghost_doc(Some(id));
                    }
                    ghosted_doc.is_some()
                } else {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.begin_transaction("MultiRegion");
                        for &(start, end) in &ranges {
                            let _ = doc.delete_range(start, end);
                        }
                        doc.commit_transaction();
                    }
                    self.do_incremental_syntax_parse();
                    true
                }
            }
            OperatorType::Yank => self.apply_to_each_region(|editor, region| {
                let Some(doc) = editor.document_manager.active_document() else {
                    return false;
                };
                let (start, end) = region.buffer_span(&doc.buffer);
                let text: Vec<Character> = doc.buffer.chars(start..end).collect();
                if !text.is_empty() {
                    editor.clipboard_ring.push(text);
                }
                true
            }),
            OperatorType::Change => {
                use crate::command::Command;
                self.enter_multi_insert(Command::EnterInsertMode, |doc, region| {
                    let (start, end) = region.buffer_span(&doc.buffer);
                    let _ = doc.delete_range(start, end);
                    start
                })
            }
        }
    }

    pub(super) fn try_run_set_aware_replace_char(&mut self, ch: char) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, end) = region.buffer_span(&doc.buffer);
            let count = end.saturating_sub(start);
            if count == 0 {
                return false;
            }
            doc.replace_repeat(start, count, ch).is_ok()
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
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        let batch = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return false;
            };
            doc.selection_set.take_for_batch(&doc.buffer)
        };
        if batch.is_empty() {
            return false;
        }
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.begin_transaction("MultiRegion");
        }
        let mut any = false;
        let mut consumed: Vec<(usize, usize)> = Vec::new();
        for region in batch {
            if consumed
                .iter()
                .any(|&(s, e)| region.anchor >= s && region.anchor < e)
            {
                continue;
            }
            let Some(doc) = self.document_manager.active_document_mut() else {
                break;
            };
            let (start, _) = region.buffer_span(&doc.buffer);
            let _ = doc.buffer.set_cursor(start);
            let Some((open_range, close_range)) =
                crate::text_objects::resolve_surround_pair(resolve_ch, &doc.buffer, count)
            else {
                continue;
            };
            if exec(self) {
                any = true;
                consumed.push((open_range.start, close_range.end));
            }
        }
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.commit_transaction();
        }
        self.do_incremental_syntax_parse();
        any
    }

    pub(super) fn try_run_set_aware_add_surround(&mut self, ch: char, delim_count: usize) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some((open, close)) = crate::text_objects::surround_strings(ch, delim_count) else {
                return false;
            };
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, end) = region.buffer_span(&doc.buffer);
            let _ = doc.buffer.set_cursor(end);
            let _ = doc.insert_str(&close);
            let _ = doc.buffer.set_cursor(start);
            let _ = doc.insert_str(&open);
            true
        })
    }

    pub(super) fn try_run_set_aware_put(&mut self, before: bool, text: &[Character]) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        let text = text.to_vec();
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, end) = region.buffer_span(&doc.buffer);
            let pos = if before { start } else { end };
            let _ = doc.buffer.set_cursor(pos);
            doc.insert_characters(&text).is_ok()
        })
    }

    pub(super) fn replay_multi_insert_at_remaining_anchors(&mut self) {
        let anchors = std::mem::take(&mut self.pending_multi_insert_anchors);
        let Some(crate::dot_repeat::DotRegister::InsertSession { commands, .. }) =
            self.dot_repeat.register().cloned()
        else {
            return;
        };
        self.dot_repeat.set_replaying(true);
        for anchor in anchors {
            if let Some(doc) = self.document_manager.active_document_mut() {
                let _ = doc.buffer.set_cursor(anchor);
            }
            for &cmd in &commands {
                self.execute_buffer_command(cmd);
            }
        }
        self.dot_repeat.set_replaying(false);
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

        let Some(source_doc_id) = self.document_manager.active_document().map(|d| d.id) else {
            return;
        };
        let regions = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.sorted())
            .unwrap_or_default();
        if regions.is_empty() {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "No regions banked".to_string(),
            );
            return;
        }

        let list_doc_id = self.document_manager.next_id();
        let mut doc = match crate::document::Document::new(list_doc_id) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        if let Some(source) = self.document_manager.active_document() {
            doc.populate_regions_buffer(&source.buffer, &regions);
        }
        doc.set_regions(source_doc_id);
        self.document_manager.add_private_document(doc);

        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let preview_win_id = self.split_tree.focused_window_id();
        let original_doc_id = self.split_tree.focused_window().document_id;
        let dir_win_id = self
            .split_tree
            .split(
                crate::split::tree::SplitDirection::Horizontal,
                preview_win_id,
                list_doc_id,
                size.rows as usize,
                size.cols as usize,
            )
            .expect("preview_win_id is the focused window, which is always a valid leaf");
        self.split_tree.set_focus(dir_win_id);
        let _ = self.document_manager.switch_to_document(list_doc_id);

        self.panel_layout = Some(crate::editor::PanelLayout {
            kind: crate::editor::PanelKind::Regions,
            dir_win_id,
            preview_win_id,
            dir_doc_id: list_doc_id,
            preview_doc_id: original_doc_id,
            original_doc_id,
        });
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
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
            .active_document()
            .map(|d| d.buffer.line_index.get_line_at(d.buffer.cursor()))
            .unwrap_or(0);
        let Some(source_doc_id) = self
            .document_manager
            .active_document()
            .and_then(|doc| doc.regions_source_doc_id())
        else {
            return false;
        };
        let Some(source) = self.document_manager.get_document_mut(source_doc_id) else {
            return false;
        };
        let sorted = source.selection_set.sorted();
        let Some(target) = sorted.get(line).copied() else {
            return false;
        };
        source.selection_set.regions.retain(|r| *r != target);
        let remaining = source.selection_set.sorted();
        let source_buf = source.buffer.clone();
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.populate_regions_buffer(&source_buf, &remaining);
        }
        true
    }
}
