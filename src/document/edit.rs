//! Document editing operations — insert, delete, and Tree-sitter incremental updates.

use super::Document;
use crate::error::RiftError;
use crate::history::{EditOperation, EditTransaction, Position, Range};
use tree_sitter::{InputEdit, Point};

impl Document {
    pub(super) fn get_point(&self, byte_offset: usize) -> Point {
        let line = self.buffer.line_index.get_line_at(byte_offset);
        let line_start = self.buffer.line_index.get_start(line).unwrap_or(0);
        let col = byte_offset.saturating_sub(line_start);
        Point {
            row: line,
            column: col,
        }
    }

    pub(super) fn byte_to_position(&self, char_idx: usize) -> Position {
        let line = self.buffer.line_index.get_line_at(char_idx);
        let line_start = self.buffer.line_index.get_start(line).unwrap_or(0);
        let col = char_idx.saturating_sub(line_start);
        Position::new(line as u32, col as u32)
    }

    pub(super) fn record_edit(&mut self, op: EditOperation, description: &str) {
        if let Some(ref mut tx) = self.current_transaction {
            tx.record(op);
        } else {
            let mut tx = EditTransaction::new(description);
            tx.record(op);
            self.history.push(tx, None);
        }
    }

    pub fn insert_char(&mut self, ch: char) -> Result<(), RiftError> {
        let start_byte = self.buffer.cursor();
        let start_position = self.get_point(start_byte);
        let history_pos = self.byte_to_position(start_byte);

        self.buffer.insert_char(ch)?;
        self.mark_dirty();

        let text = vec![crate::character::Character::from(ch)];
        let ch_str = ch.to_string();
        self.record_edit(
            EditOperation::Insert {
                position: history_pos,
                text: text.clone(),
                len: ch.len_utf8(),
            },
            &format!("Insert '{}'", if ch == '\n' { "\\n" } else { &ch_str }),
        );

        let added_bytes = ch.len_utf8();
        let new_end_byte = start_byte + added_bytes;
        let new_end_position = self.get_point(new_end_byte);

        let edit = InputEdit {
            start_byte,
            old_end_byte: start_byte,
            new_end_byte,
            start_position,
            old_end_position: start_position,
            new_end_position,
        };
        if let Some(syntax) = &mut self.syntax {
            syntax.update_tree(&edit);
        }
        Ok(())
    }

    pub fn insert_str(&mut self, s: &str) -> Result<(), RiftError> {
        let start_byte = self.buffer.cursor();
        let start_position = self.get_point(start_byte);
        let history_pos = self.byte_to_position(start_byte);

        self.buffer.insert_str(s)?;
        self.mark_dirty();

        if !s.is_empty() {
            let text: Vec<crate::character::Character> =
                s.chars().map(crate::character::Character::from).collect();
            self.record_edit(
                EditOperation::Insert {
                    position: history_pos,
                    text,
                    len: s.len(),
                },
                &format!("Insert {} chars", s.len()),
            );
        }

        let added_bytes = s.len();
        let new_end_byte = start_byte + added_bytes;
        let new_end_position = self.get_point(new_end_byte);

        let edit = InputEdit {
            start_byte,
            old_end_byte: start_byte,
            new_end_byte,
            start_position,
            old_end_position: start_position,
            new_end_position,
        };
        if let Some(syntax) = &mut self.syntax {
            syntax.update_tree(&edit);
        }
        Ok(())
    }

    pub fn delete_backward(&mut self) -> bool {
        let cursor = self.buffer.cursor();
        if cursor == 0 {
            return false;
        }

        let deleted_char = self
            .buffer
            .char_at(cursor - 1)
            .unwrap_or(crate::character::Character::from('\0'));
        let deleted_text = deleted_char.to_string();

        let history_start = self.byte_to_position(cursor - 1);
        let history_end = self.byte_to_position(cursor);

        let old_end_position = self.get_point(cursor);
        let start_position = self.get_point(cursor - 1);

        if self.buffer.delete_backward() {
            self.mark_dirty();

            self.record_edit(
                EditOperation::Delete {
                    range: Range::new(history_start, history_end),
                    deleted_text: vec![deleted_char],
                },
                &format!(
                    "Delete '{}'",
                    if deleted_text == "\n" {
                        "\\n"
                    } else {
                        &deleted_text
                    }
                ),
            );

            let start_byte = self.buffer.char_to_byte(cursor - 1);
            let old_end_byte = self.buffer.char_to_byte(cursor);

            let edit = InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte: start_byte,
                start_position,
                old_end_position,
                new_end_position: start_position,
            };
            if let Some(syntax) = &mut self.syntax {
                syntax.update_tree(&edit);
            }
            return true;
        }
        false
    }

    pub fn delete_forward(&mut self) -> bool {
        let cursor = self.buffer.cursor();
        if cursor >= self.buffer.len() {
            return false;
        }

        let deleted_char = self
            .buffer
            .char_at(cursor)
            .unwrap_or(crate::character::Character::from('\0'));
        let deleted_text = deleted_char.to_string();

        let history_start = self.byte_to_position(cursor);
        let history_end = self.byte_to_position(cursor + 1);

        let start_position = self.get_point(cursor);
        let old_end_position = self.get_point(cursor + 1);

        if self.buffer.delete_forward() {
            self.mark_dirty();

            self.record_edit(
                EditOperation::Delete {
                    range: Range::new(history_start, history_end),
                    deleted_text: vec![deleted_char],
                },
                &format!(
                    "Delete '{}'",
                    if deleted_text == "\n" {
                        "\\n"
                    } else {
                        &deleted_text
                    }
                ),
            );

            let edit = InputEdit {
                start_byte: cursor,
                old_end_byte: cursor + 1,
                new_end_byte: cursor,
                start_position,
                old_end_position,
                new_end_position: start_position,
            };
            if let Some(syntax) = &mut self.syntax {
                syntax.update_tree(&edit);
            }
            return true;
        }
        false
    }

    /// Delete a range of characters, integrating with the undo system.
    pub fn delete_range(&mut self, start: usize, end: usize) -> Result<(), RiftError> {
        if start >= end {
            return Ok(());
        }

        if end > self.buffer.len() {
            return Err(RiftError::new(
                crate::error::ErrorType::Internal,
                "INVALID_RANGE",
                format!(
                    "End position {} out of bounds (len: {})",
                    end,
                    self.buffer.len()
                ),
            ));
        }

        use crate::buffer::api::BufferView;
        let deleted_chars: Vec<crate::character::Character> =
            self.buffer.chars(start..end).collect();

        let history_start = self.byte_to_position(start);
        let history_end = self.byte_to_position(end);

        let start_position = self.get_point(start);
        let old_end_position = self.get_point(end);

        let start_byte = self.buffer.char_to_byte(start);
        let old_end_byte = self.buffer.char_to_byte(end);

        self.buffer.set_cursor(start)?;
        let count = end - start;
        for _ in 0..count {
            if !self.buffer.delete_forward() {
                break;
            }
        }

        self.mark_dirty();

        self.record_edit(
            EditOperation::Delete {
                range: Range::new(history_start, history_end),
                deleted_text: deleted_chars,
            },
            &format!("Delete {} chars", count),
        );

        let edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte: start_byte,
            start_position,
            old_end_position,
            new_end_position: start_position,
        };
        if let Some(syntax) = &mut self.syntax {
            syntax.update_tree(&edit);
        }

        Ok(())
    }
}
