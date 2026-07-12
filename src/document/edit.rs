//! Document editing operations — insert, delete, and Tree-sitter incremental updates.

use super::{AnnotationUndo, AnnotationUndoHint, Document};
use crate::character::Character;
use crate::error::RiftError;
use crate::history::{EditOperation, EditTransaction, Position, Range};

impl Document {
    /// (row, byte column) for a byte offset; `line_index` is char-indexed, so
    /// the row lookup and column both need a char/byte conversion.
    pub(super) fn get_point(&self, byte_offset: usize) -> (usize, usize) {
        let (point, _) = self.get_edit_points(byte_offset);
        point
    }

    /// Return both the (row, byte column) point and history `Position`
    /// (char column) for the same byte offset.
    pub(crate) fn get_edit_points(&self, byte_offset: usize) -> ((usize, usize), Position) {
        let char_offset = self.buffer.byte_to_char(byte_offset);
        let line = self.buffer.line_index.get_line_at(char_offset);
        let line_start_char = self.buffer.line_index.get_start(line).unwrap_or(0);
        let line_start_byte = self.buffer.char_to_byte(line_start_char);
        let byte_col = byte_offset.saturating_sub(line_start_byte);
        let char_col = char_offset.saturating_sub(line_start_char);
        (
            (line, byte_col),
            Position::new(line as u32, char_col as u32),
        )
    }

    /// `cursor_before` is the cursor position right before this edit (caller
    /// must capture it pre-mutation); used to restore the cursor on undo/redo.
    pub(super) fn record_edit(
        &mut self,
        op: EditOperation,
        description: &str,
        annotation_undo: AnnotationUndoHint,
        cursor_before: usize,
    ) {
        // Bump the monotonic edit sequence number once per applied edit.
        self.document_version = self.document_version.wrapping_add(1);
        self.pending_lsp_edits.push(op.clone());
        if let Some(ref mut tx) = self.current_transaction {
            tx.record(op);
        } else {
            // A pure insertion stores just its parameters (exactly invertible);
            // everything else takes a full pre-edit snapshot.
            let entry = match annotation_undo {
                AnnotationUndoHint::Insertion {
                    start,
                    new_end,
                    line_inserts,
                } => AnnotationUndo::Insertion {
                    start,
                    new_end,
                    line_inserts,
                },
                AnnotationUndoHint::Snapshot => {
                    AnnotationUndo::Snapshot(self.annotations.snapshot())
                }
            };
            self.annotation_undo_stack.push(entry);
            self.annotation_redo_stack.clear();
            let mut tx = EditTransaction::new(description);
            tx.cursor_before = Some(cursor_before);
            tx.record(op);
            // The buffer has already been mutated by the caller, so its
            // current cursor is exactly where this single-op edit left it.
            tx.cursor_after = Some(self.buffer.cursor());
            self.history.push(tx, None);
        }
    }

    pub fn insert_char(&mut self, ch: char) -> Result<(), RiftError> {
        let inserting_newline = ch == '\n';
        // Track line shifts for line-anchored annotations in every buffer (not
        // just directories): diagnostics and line adornments must move with edits.
        let line_before_insert = if inserting_newline {
            Some(self.buffer.get_line())
        } else {
            None
        };

        let cursor_before = self.buffer.cursor();
        let start_byte = self.buffer.char_to_byte(cursor_before);
        let (start_position, history_pos) = self.get_edit_points(start_byte);

        self.buffer.insert_char(ch)?;

        let added_bytes = ch.len_utf8();
        let new_end_byte = start_byte + added_bytes;

        // Pure insertion: undo replays the exact inverse shift. A newline also
        // shifts line anchors at `before_line + 1`, undone in lockstep.
        let line_inserts = match line_before_insert {
            Some(before_line) => vec![before_line + 1],
            None => Vec::new(),
        };

        let text = vec![Character::from(ch)];
        let ch_str = ch.to_string();
        self.record_edit(
            EditOperation::Insert {
                position: history_pos,
                text: text.clone(),
                len: ch.len_utf8(),
            },
            &format!("Insert '{}'", if ch == '\n' { "\\n" } else { &ch_str }),
            AnnotationUndoHint::Insertion {
                start: start_byte,
                new_end: new_end_byte,
                line_inserts,
            },
            cursor_before,
        );

        let new_end_position = self.get_point(new_end_byte);

        if let Some(syntax) = &mut self.syntax {
            syntax.notify_edit(
                start_byte,
                start_byte,
                new_end_byte,
                start_position,
                start_position,
                new_end_position,
            );
        }

        // Maintain byte-offset annotation markers for this edit.
        self.annotations
            .on_edit(start_byte, start_byte, new_end_byte);

        // Update directory annotation line numbers when a newline was inserted.
        if let Some(before_line) = line_before_insert {
            self.annotations.on_line_inserted(before_line + 1);
        }

        Ok(())
    }

    pub fn insert_str(&mut self, s: &str) -> Result<(), RiftError> {
        // Count newlines before inserting so we can update annotation line numbers
        // (all buffers, so line-anchored annotations track multi-line inserts).
        let newline_count = s.chars().filter(|&c| c == '\n').count();
        let line_before_insert = if newline_count > 0 {
            Some(self.buffer.get_line())
        } else {
            None
        };

        let cursor_before = self.buffer.cursor();
        let start_byte = self.buffer.char_to_byte(cursor_before);
        let (start_position, history_pos) = self.get_edit_points(start_byte);

        self.buffer.insert_str(s)?;

        let added_bytes = s.len();
        let new_end_byte = start_byte + added_bytes;

        // Pure insertion: one line-anchor shift per inserted newline, applied
        // (and later inverted) at before_line+1, +2, ... in order.
        let line_inserts: Vec<usize> = match line_before_insert {
            Some(before_line) => (0..newline_count).map(|i| before_line + 1 + i).collect(),
            None => Vec::new(),
        };

        if !s.is_empty() {
            let text: Vec<Character> = s.chars().map(Character::from).collect();
            self.record_edit(
                EditOperation::Insert {
                    position: history_pos,
                    text,
                    len: s.len(),
                },
                &format!("Insert {} chars", s.len()),
                AnnotationUndoHint::Insertion {
                    start: start_byte,
                    new_end: new_end_byte,
                    line_inserts,
                },
                cursor_before,
            );
        }

        let new_end_position = self.get_point(new_end_byte);

        if let Some(syntax) = &mut self.syntax {
            syntax.notify_edit(
                start_byte,
                start_byte,
                new_end_byte,
                start_position,
                start_position,
                new_end_position,
            );
        }

        // Maintain byte-offset annotation markers for this edit.
        self.annotations
            .on_edit(start_byte, start_byte, new_end_byte);

        // Update annotation line numbers for each newline inserted.
        if let Some(before_line) = line_before_insert {
            for i in 0..newline_count {
                self.annotations.on_line_inserted(before_line + 1 + i);
            }
        }

        Ok(())
    }

    /// Insert `Character`s at the cursor, preserving raw bytes/control chars
    /// (the byte-faithful counterpart of `insert_str`).
    pub fn insert_characters(&mut self, chars: &[Character]) -> Result<(), RiftError> {
        let newline_count = chars
            .iter()
            .filter(|c| matches!(c, Character::Newline))
            .count();
        let line_before_insert = if newline_count > 0 {
            Some(self.buffer.get_line())
        } else {
            None
        };

        let cursor_before = self.buffer.cursor();
        let start_byte = self.buffer.char_to_byte(cursor_before);
        let (start_position, history_pos) = self.get_edit_points(start_byte);

        self.buffer.insert_chars(chars)?;

        let added_bytes: usize = chars.iter().map(|c| c.len_utf8()).sum();
        let new_end_byte = start_byte + added_bytes;

        let line_inserts: Vec<usize> = match line_before_insert {
            Some(before_line) => (0..newline_count).map(|i| before_line + 1 + i).collect(),
            None => Vec::new(),
        };

        if !chars.is_empty() {
            self.record_edit(
                EditOperation::Insert {
                    position: history_pos,
                    text: chars.to_vec(),
                    len: added_bytes,
                },
                &format!("Insert {} chars", chars.len()),
                AnnotationUndoHint::Insertion {
                    start: start_byte,
                    new_end: new_end_byte,
                    line_inserts,
                },
                cursor_before,
            );
        }

        let new_end_position = self.get_point(new_end_byte);

        if let Some(syntax) = &mut self.syntax {
            syntax.notify_edit(
                start_byte,
                start_byte,
                new_end_byte,
                start_position,
                start_position,
                new_end_position,
            );
        }

        self.annotations
            .on_edit(start_byte, start_byte, new_end_byte);

        if let Some(before_line) = line_before_insert {
            for i in 0..newline_count {
                self.annotations.on_line_inserted(before_line + 1 + i);
            }
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
            .unwrap_or(Character::from('\0'));

        // For directory buffers, block newline deletion to prevent line merges.
        if self.is_directory() && deleted_char == Character::Newline {
            return false;
        }

        let deleted_text = deleted_char.to_string();
        let start_byte = self.buffer.char_to_byte(cursor - 1);
        let old_end_byte = self.buffer.char_to_byte(cursor);
        let (start_position, history_start) = self.get_edit_points(start_byte);
        let (old_end_position, history_end) = self.get_edit_points(old_end_byte);

        if self.buffer.delete_backward() {
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
                AnnotationUndoHint::Snapshot,
                cursor,
            );

            if let Some(syntax) = &mut self.syntax {
                syntax.notify_edit(
                    start_byte,
                    old_end_byte,
                    start_byte,
                    start_position,
                    old_end_position,
                    start_position,
                );
            }
            self.annotations
                .on_edit(start_byte, old_end_byte, start_byte);
            // Deleting a newline merges the next line up: renumber line anchors.
            if deleted_char == Character::Newline {
                self.annotations.on_lines_deleted(start_position.0 + 1, 1);
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

        let deleted_char = self.buffer.char_at(cursor).unwrap_or(Character::from('\0'));

        // For directory buffers, block newline deletion to prevent line merges.
        if self.is_directory() && deleted_char == Character::Newline {
            return false;
        }

        let deleted_text = deleted_char.to_string();
        let start_byte = self.buffer.char_to_byte(cursor);
        let old_end_byte = self.buffer.char_to_byte(cursor + 1);
        let (start_position, history_start) = self.get_edit_points(start_byte);
        let (old_end_position, history_end) = self.get_edit_points(old_end_byte);

        if self.buffer.delete_forward() {
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
                AnnotationUndoHint::Snapshot,
                cursor,
            );

            if let Some(syntax) = &mut self.syntax {
                syntax.notify_edit(
                    start_byte,
                    old_end_byte,
                    start_byte,
                    start_position,
                    old_end_position,
                    start_position,
                );
            }
            self.annotations
                .on_edit(start_byte, old_end_byte, start_byte);
            // Deleting a newline merges the next line up: renumber line anchors.
            if deleted_char == Character::Newline {
                self.annotations.on_lines_deleted(start_position.0 + 1, 1);
            }
            return true;
        }
        false
    }

    /// Replace `count` chars at `pos` with `new_chars`
    pub fn replace_chars(
        &mut self,
        pos: usize,
        count: usize,
        new_chars: &[Character],
    ) -> Result<(), RiftError> {
        let end = pos + count;
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
        let cursor_before = self.buffer.cursor();
        let old_chars: Vec<Character> = self.buffer.chars(pos..end).collect();

        let start_byte = self.buffer.char_to_byte(pos);
        let old_end_byte = self.buffer.char_to_byte(end);
        let (start_position, history_start) = self.get_edit_points(start_byte);
        let (old_end_position, history_end) = self.get_edit_points(old_end_byte);

        self.buffer.replace_range(pos, count, new_chars);

        self.record_edit(
            EditOperation::Replace {
                range: Range::new(history_start, history_end),
                old_text: old_chars,
                new_text: new_chars.to_vec(),
            },
            "Replace",
            AnnotationUndoHint::Snapshot,
            cursor_before,
        );

        let new_end_byte = self.buffer.char_to_byte(pos + new_chars.len());
        let new_end_position = self.get_point(new_end_byte);
        if let Some(syntax) = &mut self.syntax {
            syntax.notify_edit(
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position,
                old_end_position,
                new_end_position,
            );
        }
        self.annotations
            .on_edit(start_byte, old_end_byte, new_end_byte);

        Ok(())
    }

    /// Replace `count` chars at `pos` with `count` copies of `ch`.
    /// Allocates once, creates one add-buffer piece, and one undo record.
    pub fn replace_repeat(&mut self, pos: usize, count: usize, ch: char) -> Result<(), RiftError> {
        let fill: Vec<Character> = vec![Character::from(ch); count];
        self.replace_chars(pos, count, &fill)
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
        let cursor_before = self.buffer.cursor();
        let deleted_chars: Vec<Character> = self.buffer.chars(start..end).collect();

        // Track deleted lines for line-anchored annotations in every buffer.
        let deleted_line_info: Option<(usize, usize)> = {
            let newline_count = deleted_chars
                .iter()
                .filter(|&&c| c == Character::Newline)
                .count();
            if newline_count > 0 {
                let first_line = self.buffer.line_index.get_line_at(start);
                Some((first_line, newline_count))
            } else {
                None
            }
        };

        let start_byte = self.buffer.char_to_byte(start);
        let old_end_byte = self.buffer.char_to_byte(end);
        let (start_position, history_start) = self.get_edit_points(start_byte);
        let (old_end_position, history_end) = self.get_edit_points(old_end_byte);

        let count = end - start;
        self.buffer.delete_range(start, count);
        let new_cursor = start.min(self.buffer.len());
        let _ = self.buffer.set_cursor(new_cursor);

        self.record_edit(
            EditOperation::Delete {
                range: Range::new(history_start, history_end),
                deleted_text: deleted_chars,
            },
            &format!("Delete {} chars", count),
            AnnotationUndoHint::Snapshot,
            cursor_before,
        );

        if let Some(syntax) = &mut self.syntax {
            syntax.notify_edit(
                start_byte,
                old_end_byte,
                start_byte,
                start_position,
                old_end_position,
                start_position,
            );
        }
        self.annotations
            .on_edit(start_byte, old_end_byte, start_byte);

        // Update directory annotation line numbers after the buffer mutation.
        if let Some((first_line, newline_count)) = deleted_line_info {
            self.annotations.on_lines_deleted(first_line, newline_count);
        }

        Ok(())
    }
}
