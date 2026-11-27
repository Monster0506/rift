use super::super::{AnnotationUndo, Document};
use super::{Origin, Resolved};
use crate::character::Character;
use crate::history::{EditOperation, EditTransaction, Position};

#[derive(Clone, Copy)]
pub(crate) enum EditSession {
    Insert,
    Replace,
    Change,
    MultiInsert,
    MultiRegion,
}

impl Document {
    pub(crate) fn edit_group<R>(
        &mut self,
        description: impl Into<String>,
        edit: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.begin_transaction(description);
        let result = edit(self);
        self.commit_transaction();
        result
    }

    pub(crate) fn start_edit_session(&mut self, session: EditSession) {
        self.begin_transaction(match session {
            EditSession::Insert => "Insert",
            EditSession::Replace => "Replace",
            EditSession::Change => "Change",
            EditSession::MultiInsert => "MultiInsert",
            EditSession::MultiRegion => "MultiRegion",
        });
    }

    pub(crate) fn finish_edit_session(&mut self) {
        self.commit_transaction();
    }

    pub(in crate::document) fn begin_transaction(&mut self, description: impl Into<String>) {
        if self.transaction_depth == 0 {
            self.commit_pending_ghost();
        }
        self.transaction_depth += 1;
        if self.transaction_depth == 1 {
            let mut tx = EditTransaction::new(description);
            tx.cursor_before = Some(self.buffer.cursor());
            self.current_transaction = Some(tx);
            self.pending_annotation_snapshot = Some(self.annotations.snapshot());
        }
    }

    pub(in crate::document) fn commit_transaction(&mut self) {
        if self.transaction_depth == 0 {
            return;
        }
        self.transaction_depth -= 1;
        if self.transaction_depth == 0 {
            if let Some(mut tx) = self.current_transaction.take() {
                if tx.is_empty() {
                    self.pending_annotation_snapshot = None;
                } else {
                    tx.cursor_after = Some(self.buffer.cursor());
                    self.history.push(tx, None);
                    if let Some(snapshot) = self.pending_annotation_snapshot.take() {
                        self.annotation_undo_stack
                            .push(AnnotationUndo::Snapshot(snapshot));
                        self.annotation_redo_stack.clear();
                    }
                }
            }
            self.settle_syntax();
        }
    }

    pub(crate) fn undo(&mut self) -> bool {
        self.commit_pending_ghost();
        if !self.history.can_undo() {
            return false;
        }
        let Some((inverse_ops, cursor_before)) = self
            .history
            .current_transaction()
            .map(|tx| (tx.inverse(), tx.cursor_before))
        else {
            return false;
        };

        for op in &inverse_ops {
            self.replay(op);
        }

        if let Some(offset) = cursor_before {
            let _ = self.set_cursor(
                offset.min(self.buffer.len()),
                crate::document::ops::CursorCause::User,
            );
        }

        self.history.undo();

        if let Some(entry) = self.annotation_undo_stack.pop() {
            self.restore_annotations_for_undo(entry);
        }

        self.clear_selection();
        self.settle_syntax();
        true
    }

    pub(crate) fn redo(&mut self) -> bool {
        self.commit_pending_ghost();
        if !self.history.can_redo() {
            return false;
        }
        if self.history.redo().is_none() {
            return false;
        }

        let Some((ops, cursor_after)) = self
            .history
            .current_transaction()
            .map(|tx| (tx.ops.clone(), tx.cursor_after))
        else {
            return false;
        };

        for op in &ops {
            self.replay(op);
        }

        if let Some(offset) = cursor_after {
            let _ = self.set_cursor(
                offset.min(self.buffer.len()),
                crate::document::ops::CursorCause::User,
            );
        }

        if let Some(entry) = self.annotation_redo_stack.pop() {
            self.restore_annotations_for_redo(entry);
        }

        self.clear_selection();
        self.settle_syntax();
        true
    }

    fn replay(&mut self, op: &EditOperation) {
        let (start, end, text) = match op {
            EditOperation::Insert { position, text, .. } => {
                let at = self.position_to_char_offset(*position);
                (at, at, text.clone())
            }
            EditOperation::Delete { range, .. } => (
                self.position_to_char_offset(range.start),
                self.position_to_char_offset(range.end),
                Vec::new(),
            ),
            EditOperation::Replace {
                range, new_text, ..
            } => (
                self.position_to_char_offset(range.start),
                self.position_to_char_offset(range.end),
                new_text.clone(),
            ),
            EditOperation::BlockChange {
                range, new_content, ..
            } => (
                self.position_to_char_offset(range.start),
                self.position_to_char_offset(range.end),
                new_content.join(&Character::Newline),
            ),
        };
        debug_assert!(
            end >= start,
            "stale history Position: range end before start"
        );
        let _ = self.apply_resolved(
            Resolved {
                start,
                end: end.max(start),
                text,
            },
            Origin::Replay,
        );
    }

    fn position_to_char_offset(&self, pos: Position) -> usize {
        let resolved = self.buffer.line_index.get_start(pos.line as usize);
        debug_assert!(
            resolved.is_some(),
            "stale history Position: line {} no longer exists in buffer",
            pos.line
        );
        resolved.unwrap_or(0) + pos.col as usize
    }

    fn restore_annotations_for_undo(&mut self, entry: AnnotationUndo) {
        match entry {
            AnnotationUndo::Snapshot(snapshot) => {
                self.annotation_redo_stack
                    .push(AnnotationUndo::Snapshot(self.annotations.snapshot()));
                self.annotations.restore(snapshot);
            }
            AnnotationUndo::Insertion {
                start,
                new_end,
                line_inserts,
            } => {
                self.annotation_redo_stack.push(AnnotationUndo::Insertion {
                    start,
                    new_end,
                    line_inserts: line_inserts.clone(),
                });
                self.annotations.on_edit(start, new_end, start);
                for at in line_inserts.iter().rev() {
                    self.annotations.undo_line_inserted(*at);
                }
            }
        }
    }

    fn restore_annotations_for_redo(&mut self, entry: AnnotationUndo) {
        match entry {
            AnnotationUndo::Snapshot(snapshot) => {
                self.annotation_undo_stack
                    .push(AnnotationUndo::Snapshot(self.annotations.snapshot()));
                self.annotations.restore(snapshot);
            }
            AnnotationUndo::Insertion {
                start,
                new_end,
                line_inserts,
            } => {
                self.annotation_undo_stack.push(AnnotationUndo::Insertion {
                    start,
                    new_end,
                    line_inserts: line_inserts.clone(),
                });
                self.annotations.on_edit(start, start, new_end);
                for at in &line_inserts {
                    self.annotations.on_line_inserted(*at);
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) const fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub(crate) fn checkpoint(&mut self) {
        use crate::buffer::api::BufferView;
        use crate::history::DocumentSnapshot;

        let full_text: Vec<Character> = self.buffer.chars(0..self.buffer.len()).collect();
        let snapshot = DocumentSnapshot::new(full_text, self.annotations.snapshot());
        self.history.checkpoint(snapshot);
    }

    pub(crate) fn goto_seq(&mut self, target: u64) -> Result<(), crate::history::UndoError> {
        self.commit_pending_ghost();
        let replay_path = self.history.goto_seq(target)?;

        if let Some(snapshot) = &replay_path.snapshot_restore {
            self.replace_content(&snapshot.full_text, super::ContentKind::Render);
            self.annotations.restore(snapshot.annotations.clone());
            self.annotation_undo_stack.clear();
            self.annotation_redo_stack.clear();
            for tx in &replay_path.redo_ops {
                for op in &tx.ops {
                    self.replay(op);
                }
            }
            self.clear_selection();
            self.settle_syntax();
            return Ok(());
        }

        for tx in &replay_path.undo_ops {
            for op in tx.inverse() {
                self.replay(&op);
            }
            if let Some(entry) = self.annotation_undo_stack.pop() {
                self.restore_annotations_for_undo(entry);
            }
        }
        for tx in &replay_path.redo_ops {
            for op in &tx.ops {
                self.replay(op);
            }
            if let Some(entry) = self.annotation_redo_stack.pop() {
                self.restore_annotations_for_redo(entry);
            }
        }
        self.clear_selection();
        self.settle_syntax();
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::document) fn get_changed_line_for_seq(&self, seq: u64) -> Option<usize> {
        let node = self.history.nodes.get(&seq)?;
        let op = node.transaction.ops.first()?;

        let line = match op {
            EditOperation::Insert { position, .. } => position.line,
            EditOperation::Delete { range, .. }
            | EditOperation::Replace { range, .. }
            | EditOperation::BlockChange { range, .. } => range.start.line,
        };

        Some(line as usize)
    }
}
