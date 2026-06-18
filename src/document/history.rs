//! Document undo/redo: transactions, history navigation, replay.

use super::Document;
use crate::buffer::TextBuffer;
use crate::history::{EditOperation, EditTransaction, Position, ReplayPath};
use tree_sitter::InputEdit;

impl Document {
    /// Start a transaction for grouping multiple edits.
    pub fn begin_transaction(&mut self, description: impl Into<String>) {
        self.transaction_depth += 1;
        if self.transaction_depth == 1 {
            let mut tx = EditTransaction::new(description);
            tx.cursor_before = Some(self.buffer.cursor());
            self.current_transaction = Some(tx);
            // Capture the full annotation state before the transaction's edits.
            self.pending_annotation_snapshot = Some(self.annotations.snapshot());
        }
    }

    /// Commit the current transaction to undo history.
    pub fn commit_transaction(&mut self) {
        if self.transaction_depth == 0 {
            return;
        }
        self.transaction_depth -= 1;
        if self.transaction_depth == 0 {
            if let Some(tx) = self.current_transaction.take() {
                if !tx.is_empty() {
                    self.history.push(tx, None);
                    if let Some(snapshot) = self.pending_annotation_snapshot.take() {
                        self.annotation_undo_stack
                            .push(super::AnnotationUndo::Snapshot(snapshot));
                        self.annotation_redo_stack.clear();
                    }
                } else {
                    self.pending_annotation_snapshot = None;
                }
            }
        }
    }

    /// Undo the last edit. Returns true if successful.
    pub fn undo(&mut self) -> bool {
        if !self.history.can_undo() {
            return false;
        }
        let (inverse_ops, cursor_before) = if let Some(tx) = self.history.current_transaction() {
            (tx.inverse(), tx.cursor_before)
        } else {
            return false;
        };

        for op in inverse_ops {
            self.apply_operation_with_tree_update(&op);
        }

        // Restore cursor to where it was before the command, not wherever
        // the last inverse op left it (insert_chars advances past inserted text).
        if let Some(offset) = cursor_before {
            let _ = self.buffer.set_cursor(offset.min(self.buffer.len()));
        }

        self.history.undo();
        self.mark_dirty();

        // Restore the pre-edit annotation positions; record how to re-apply
        // (redo) the edit. The buffer was reverted above, so these now align.
        if let Some(entry) = self.annotation_undo_stack.pop() {
            self.restore_annotations_for_undo(entry);
        }

        true
    }

    /// Redo the last undone edit. Returns true if successful.
    pub fn redo(&mut self) -> bool {
        if !self.history.can_redo() {
            return false;
        }
        if self.history.redo().is_none() {
            return false;
        }

        let ops = if let Some(tx) = self.history.current_transaction() {
            tx.ops.clone()
        } else {
            return false;
        };

        for op in ops {
            self.apply_operation_with_tree_update(&op);
        }

        // Re-apply the post-edit annotation positions; record how to undo
        // again. The buffer was re-applied above, so these now align.
        if let Some(entry) = self.annotation_redo_stack.pop() {
            self.restore_annotations_for_redo(entry);
        }

        self.mark_dirty();

        true
    }

    /// Apply an edit operation (used by undo/redo), informing the syntax tree
    /// via `InputEdit` so it stays valid for incremental reuse.
    fn apply_operation_with_tree_update(&mut self, op: &EditOperation) {
        let (start_char, old_end_char) = match op {
            EditOperation::Insert { position, .. } => {
                let c = self.position_to_char_offset(*position);
                (c, c)
            }
            EditOperation::Delete { range, .. }
            | EditOperation::Replace { range, .. }
            | EditOperation::BlockChange { range, .. } => (
                self.position_to_char_offset(range.start),
                self.position_to_char_offset(range.end),
            ),
        };
        let start_byte = self.buffer.char_to_byte(start_char);
        let old_end_byte = self.buffer.char_to_byte(old_end_char);
        let start_position = self.get_point(start_byte);
        let old_end_position = self.get_point(old_end_byte);

        Self::apply_operation_to_buffer(&mut self.buffer, op);

        let inserted_chars = match op {
            EditOperation::Insert { text, .. } => text.len(),
            EditOperation::Delete { .. } => 0,
            EditOperation::Replace { new_text, .. } => new_text.len(),
            EditOperation::BlockChange { new_content, .. } => {
                new_content.iter().map(|line| line.len()).sum::<usize>()
                    + new_content.len().saturating_sub(1)
            }
        };
        let new_end_byte = self.buffer.char_to_byte(start_char + inserted_chars);
        let new_end_position = self.get_point(new_end_byte);

        let edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        };
        if let Some(syntax) = &mut self.syntax {
            syntax.update_tree(&edit);
        }
    }

    /// Char offset of a `Position` (line, char-column) in the current buffer.
    fn position_to_char_offset(&self, pos: Position) -> usize {
        let line_start = self
            .buffer
            .line_index
            .get_start(pos.line as usize)
            .unwrap_or(0);
        line_start + pos.col as usize
    }

    /// Restore the pre-edit annotation state for an undo, pushing the matching
    /// redo entry. The buffer has already been reverted by the caller.
    fn restore_annotations_for_undo(&mut self, entry: super::AnnotationUndo) {
        use super::AnnotationUndo;
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
                // Redo will re-apply this exact forward insertion.
                self.annotation_redo_stack.push(AnnotationUndo::Insertion {
                    start,
                    new_end,
                    line_inserts: line_inserts.clone(),
                });
                // Invert the byte shift, then each line insert in reverse order
                // (the inverse of a composition is the reversed inverses).
                self.annotations.on_edit(start, new_end, start);
                for at in line_inserts.iter().rev() {
                    self.annotations.undo_line_inserted(*at);
                }
            }
        }
    }

    /// Re-apply the post-edit annotation state for a redo, pushing the matching
    /// undo entry. The buffer has already been re-applied by the caller.
    fn restore_annotations_for_redo(&mut self, entry: super::AnnotationUndo) {
        use super::AnnotationUndo;
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
                // Re-apply the forward insertion exactly as the edit did.
                self.annotations.on_edit(start, start, new_end);
                for at in &line_inserts {
                    self.annotations.on_line_inserted(*at);
                }
            }
        }
    }

    /// Apply an edit operation to a given buffer.
    fn apply_operation_to_buffer(buffer: &mut TextBuffer, op: &EditOperation) {
        match op {
            EditOperation::Insert { position, text, .. } => {
                let line_start = buffer
                    .line_index
                    .get_start(position.line as usize)
                    .unwrap_or(0);
                let char_offset = line_start + position.col as usize;
                let _ = buffer.set_cursor(char_offset);
                let _ = buffer.insert_chars(text);
            }
            EditOperation::Delete { range, .. } => {
                let start_line_start = buffer
                    .line_index
                    .get_start(range.start.line as usize)
                    .unwrap_or(0);
                let start_offset = start_line_start + range.start.col as usize;
                let end_line_start = buffer
                    .line_index
                    .get_start(range.end.line as usize)
                    .unwrap_or(0);
                let end_offset = end_line_start + range.end.col as usize;

                let count = end_offset.saturating_sub(start_offset);
                buffer.delete_range(start_offset, count);
                let _ = buffer.set_cursor(start_offset.min(buffer.len()));
            }
            EditOperation::Replace {
                range, new_text, ..
            } => {
                let start_line_start = buffer
                    .line_index
                    .get_start(range.start.line as usize)
                    .unwrap_or(0);
                let start_offset = start_line_start + range.start.col as usize;
                let end_line_start = buffer
                    .line_index
                    .get_start(range.end.line as usize)
                    .unwrap_or(0);
                let end_offset = end_line_start + range.end.col as usize;
                let count = end_offset.saturating_sub(start_offset);
                buffer.replace_range(start_offset, count, new_text);
            }
            EditOperation::BlockChange {
                range, new_content, ..
            } => {
                let start_line_start = buffer
                    .line_index
                    .get_start(range.start.line as usize)
                    .unwrap_or(0);
                let start_offset = start_line_start + range.start.col as usize;
                let end_line_start = buffer
                    .line_index
                    .get_start(range.end.line as usize)
                    .unwrap_or(0);
                let end_offset = end_line_start + range.end.col as usize;

                let count = end_offset.saturating_sub(start_offset);
                buffer.delete_range(start_offset, count);
                let _ = buffer.set_cursor(start_offset.min(buffer.len()));
                for (i, line) in new_content.iter().enumerate() {
                    if i > 0 {
                        let _ = buffer.insert_character(crate::character::Character::Newline);
                    }
                    let _ = buffer.insert_chars(line);
                }
            }
        }
    }

    fn apply_replay_path_to_buffer(buffer: &mut TextBuffer, path: &ReplayPath) {
        for tx in &path.undo_ops {
            for op in tx.inverse() {
                Self::apply_operation_to_buffer(buffer, &op);
            }
        }
        for tx in &path.redo_ops {
            for op in &tx.ops {
                Self::apply_operation_to_buffer(buffer, op);
            }
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Create a checkpoint at the current position
    pub fn checkpoint(&mut self) {
        use crate::buffer::api::BufferView;
        use crate::history::DocumentSnapshot;

        let full_text: Vec<crate::character::Character> =
            self.buffer.chars(0..self.buffer.len()).collect();
        let snapshot = DocumentSnapshot::new(full_text);
        self.history.checkpoint(snapshot);
    }

    /// Navigate to a specific edit sequence in the undo tree
    pub fn goto_seq(&mut self, target: u64) -> Result<(), crate::history::UndoError> {
        let replay_path = self.history.goto_seq(target)?;
        for tx in &replay_path.undo_ops {
            for op in tx.inverse() {
                self.apply_operation_with_tree_update(&op);
            }
        }
        for tx in &replay_path.redo_ops {
            for op in &tx.ops {
                self.apply_operation_with_tree_update(op);
            }
        }
        self.mark_dirty();
        Ok(())
    }

    /// Get a preview of the document at a specific edit sequence
    pub fn preview_at_seq(&self, seq: u64) -> Result<String, crate::history::UndoError> {
        let path = self
            .history
            .compute_replay_path(self.history.current_seq(), seq)?;
        let mut preview_buffer = self.buffer.clone();
        Self::apply_replay_path_to_buffer(&mut preview_buffer, &path);
        Ok(preview_buffer.to_string())
    }

    /// Get the line number where an edit occurred for a specific sequence
    pub fn get_changed_line_for_seq(&self, seq: u64) -> Option<usize> {
        use crate::history::EditOperation;

        let node = self.history.nodes.get(&seq)?;
        let op = node.transaction.ops.first()?;

        let line = match op {
            EditOperation::Insert { position, .. } => position.line,
            EditOperation::Delete { range, .. } => range.start.line,
            EditOperation::Replace { range, .. } => range.start.line,
            EditOperation::BlockChange { range, .. } => range.start.line,
        };

        Some(line as usize)
    }
}
