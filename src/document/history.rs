//! Document undo/redo — transactions, history navigation, replay.

use super::Document;
use crate::buffer::TextBuffer;
use crate::history::{EditOperation, EditTransaction, ReplayPath};

impl Document {
    /// Start a transaction for grouping multiple edits
    pub fn begin_transaction(&mut self, description: impl Into<String>) {
        self.current_transaction = Some(EditTransaction::new(description));
    }

    /// Commit the current transaction to undo history
    pub fn commit_transaction(&mut self) {
        if let Some(tx) = self.current_transaction.take() {
            if !tx.is_empty() {
                self.history.push(tx, None);
            }
        }
    }

    /// Undo the last edit. Returns true if successful.
    pub fn undo(&mut self) -> bool {
        if !self.history.can_undo() {
            return false;
        }
        let inverse_ops = if let Some(tx) = self.history.current_transaction() {
            tx.inverse()
        } else {
            return false;
        };

        for op in inverse_ops {
            self.apply_operation(&op);
        }

        self.history.undo();
        self.mark_dirty();

        if let Some(syntax) = &mut self.syntax {
            syntax.invalidate_trees();
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
            self.apply_operation(&op);
        }

        self.mark_dirty();

        if let Some(syntax) = &mut self.syntax {
            syntax.invalidate_trees();
        }
        true
    }

    /// Apply an edit operation to this document's buffer (used by undo/redo).
    pub(crate) fn apply_operation(&mut self, op: &EditOperation) {
        Self::apply_operation_to_buffer(&mut self.buffer, op);
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
                buffer.delete_range(start_offset, count);
                let _ = buffer.set_cursor(start_offset.min(buffer.len()));
                let _ = buffer.insert_chars(new_text);
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
        Self::apply_replay_path_to_buffer(&mut self.buffer, &replay_path);
        self.mark_dirty();

        if let Some(syntax) = &mut self.syntax {
            syntax.tree = None;
        }
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
