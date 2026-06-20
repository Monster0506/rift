//! Region-set navigation (`n`/`N` when banked regions exist).

use super::Editor;
use crate::term::TerminalBackend;

/// Char offset of the start of `row`.
fn line_start_offset(buf: &crate::buffer::TextBuffer, row: usize) -> usize {
    buf.line_index.get_start(row).unwrap_or(0)
}

/// Char offset of the end of `row` (trailing newline or buffer's end).
/// Mirrors `clipboard::capture_text`'s Linewise guarded pattern.
fn line_end_offset(buf: &crate::buffer::TextBuffer, row: usize) -> usize {
    if row + 1 < buf.get_total_lines() {
        buf.line_index.get_start(row + 1).unwrap_or(buf.len()).saturating_sub(1)
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
    /// `<Space>`: grow the active Visual region to the smallest enclosing
    /// candidate that's strictly larger than the current span. Pushes the
    /// prior extent onto `expand_history` first (Task 24 pops it).
    pub(super) fn expand_active_region(&mut self) -> bool {
        let Some(anchor) = self.visual_anchor else { return false };
        let Some(doc) = self.document_manager.active_document() else { return false };
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
            // Clamp: a last line with no trailing newline can overshoot by one
            // (same case clipboard::capture_text already guards).
            let e = (range.anchor.max(range.new_cursor) + end_offset).min(doc.buffer.len());
            let strictly_larger = s <= current.0 && e >= current.1 && (s < current.0 || e > current.1);
            if !strictly_larger {
                continue;
            }
            if best.is_none_or(|(bs, be)| (e - s) < (be - bs)) {
                best = Some((s, e));
            }
        }

        let Some((new_start, new_end)) = best else { return false };
        self.expand_history.push(current);
        self.visual_anchor = Some(new_start);
        if let Some(doc) = self.document_manager.active_document_mut() {
            let _ = doc.buffer.set_cursor(new_end.saturating_sub(1));
        }
        true
    }

    /// `n`/`N` when the `SelectionSet` is non-empty: cycle the cursor
    /// between banked regions instead of repeat-find/search (design.md S3,
    /// resolved as context-sensitive per this codebase's existing n/N bindings).
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

    /// Run `f` once per banked region, highest-offset-first, inside one
    /// transaction so the whole batch undoes as a single step. Returns
    /// `false` without doing anything if the set is empty.
    pub(super) fn apply_to_each_region<F>(&mut self, mut f: F) -> bool
    where
        F: FnMut(&mut Self, crate::selection::Region) -> bool,
    {
        let batch = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return false;
            };
            doc.selection_set.take_for_batch()
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
        any
    }

    /// Enter Insert mode at the highest-offset anchor (`anchor_for` may mutate
    /// the doc, e.g. deleting the region for `c`); records via dot-repeat so exit replays at remaining anchors.
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
            let batch = doc.selection_set.take_for_batch();
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

    /// `i`/`a`/`I`/`A`/`o`/`O` against non-empty `SelectionSet`: enter
    /// multi-insert instead of single-cursor path; `false` if empty or unhandled.
    pub(super) fn try_multi_insert_for_command(&mut self, entry: crate::command::Command) -> bool {
        use crate::command::Command;

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

    /// `d`/`y` (and `c`, Task 14) against a non-empty `SelectionSet`: run the
    /// whole banked set as one batch instead of entering `OperatorPending`
    /// for a single motion. Returns `false` if the set is empty so the
    /// caller falls through to today's single-cursor behavior unchanged.
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
            OperatorType::Delete => self.apply_to_each_region(|editor, region| {
                let Some(doc) = editor.document_manager.active_document_mut() else {
                    return false;
                };
                let (start, end) = region.buffer_span(&doc.buffer);
                let text: String = doc.buffer.chars(start..end).map(|c| c.to_char_lossy()).collect();
                if doc.delete_range(start, end).is_err() {
                    return false;
                }
                if !text.is_empty() {
                    editor.clipboard_ring.push(text);
                }
                true
            }),
            OperatorType::Yank => self.apply_to_each_region(|editor, region| {
                let Some(doc) = editor.document_manager.active_document() else {
                    return false;
                };
                let (start, end) = region.buffer_span(&doc.buffer);
                let text: String = doc.buffer.chars(start..end).map(|c| c.to_char_lossy()).collect();
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

    /// `r<ch>` against a non-empty `SelectionSet`: fill each region's exact
    /// range with `ch`, ignoring any numeric count (design.md S5.3).
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

    /// `sd<ch>` against a non-empty `SelectionSet`: reuse the existing
    /// single-cursor `Command::DeleteSurround` resolution once per region.
    pub(super) fn try_run_set_aware_delete_surround(&mut self, ch: char, count: usize) -> bool {
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
            let (start, _) = region.buffer_span(&doc.buffer);
            let _ = doc.buffer.set_cursor(start);
            editor.execute_buffer_command(crate::command::Command::DeleteSurround(ch, count))
        })
    }

    /// `sc<from><to>` against a non-empty `SelectionSet`: same pattern as
    /// `try_run_set_aware_delete_surround`.
    pub(super) fn try_run_set_aware_change_surround(
        &mut self,
        from: char,
        to: char,
        count: usize,
    ) -> bool {
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
            let (start, _) = region.buffer_span(&doc.buffer);
            let _ = doc.buffer.set_cursor(start);
            editor.execute_buffer_command(crate::command::Command::ChangeSurround(from, to, count))
        })
    }

    /// `sg<ch>` against a non-empty `SelectionSet`: the region supplies the
    /// range directly, so this mirrors `Command::AddSurround`'s body instead
    /// of routing through `compute_motion_range`.
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

    /// `p`/`P` (and `PutSystemClipboard`) against a non-empty `SelectionSet`:
    /// insert the same `text` at every region, after its end for `p`, before its start for `P`. Non-destructive (design.md S5.7).
    pub(super) fn try_run_set_aware_put(&mut self, before: bool, text: &str) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        let text = text.to_string();
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, end) = region.buffer_span(&doc.buffer);
            let pos = if before { start } else { end };
            let _ = doc.buffer.set_cursor(pos);
            doc.insert_str(&text).is_ok()
        })
    }

    /// Replay the just-finished Insert session at every pending anchor; must run
    /// before the outer `MultiInsert` transaction commits so all anchors share the live-typed undo step (S5.8).
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

    /// Finalize the accumulated selection-building actions into a
    /// `DotRegister::RegionBuildSession`, if anything was recorded.
    pub(super) fn finish_region_build(&mut self, follow_up: Option<crate::action::Action>) {
        if self.region_build_recording.is_empty() {
            return;
        }
        let actions = std::mem::take(&mut self.region_build_recording);
        if !self.dot_repeat.is_replaying() {
            self.dot_repeat.record_region_build_session(actions, follow_up);
        }
    }
}
