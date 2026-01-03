//! Document management
//! Encapsulates buffer + file metadata for multi-buffer support

use crate::buffer::TextBuffer;
use crate::error::{ErrorType, RiftError};
use crate::history::{EditOperation, EditTransaction, Position, Range, UndoTree};
use crate::syntax::Syntax;
use std::io;
use std::path::{Path, PathBuf};
use tree_sitter::{InputEdit, Point};

pub mod definitions;
use definitions::DocumentOptions;

/// Unique identifier for documents
pub type DocumentId = u64;

/// Line ending types supported by Rift
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix line endings (\n)
    LF,
    /// Windows line endings (\r\n)
    CRLF,
}

impl LineEnding {
    /// Get the byte sequence for this line ending
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            LineEnding::LF => b"\n",
            LineEnding::CRLF => b"\r\n",
        }
    }
}

/// Document combining buffer and file metadata
pub struct Document {
    /// Unique document identifier
    pub id: DocumentId,
    /// Text buffer
    pub buffer: TextBuffer,
    /// Document-specific options (line endings, etc.)
    pub options: DocumentOptions,
    /// File path (None if new/unsaved)
    file_path: Option<PathBuf>,
    /// Current revision number (incremented on edits)
    revision: u64,
    /// Revision of last save
    last_saved_revision: u64,
    /// Read-only flag (for permissions or :view mode)
    pub is_read_only: bool,
    /// Syntax highlighting/parsing
    pub syntax: Option<Syntax>,
    /// Undo/redo history tree
    pub history: UndoTree,
    /// Current transaction for grouping edits
    current_transaction: Option<EditTransaction>,
}

impl Document {
    /// Create a new empty document
    pub fn new(id: DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            id,
            buffer,
            options: DocumentOptions::default(),
            file_path: None,
            revision: 0,
            last_saved_revision: 0,
            is_read_only: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
        })
    }

    /// Load document from file
    pub fn from_file(id: DocumentId, path: impl AsRef<Path>) -> Result<Self, RiftError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;

        // Detect line endings and normalize
        let mut line_ending = LineEnding::LF;
        let mut normalized_bytes = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                line_ending = LineEnding::CRLF;
                normalized_bytes.push(b'\n');
                i += 2;
            } else {
                normalized_bytes.push(bytes[i]);
                i += 1;
            }
        }

        let mut buffer =
            TextBuffer::new(normalized_bytes.len().max(4096)).map_err(io::Error::other)?;

        buffer
            .insert_bytes(&normalized_bytes)
            .map_err(io::Error::other)?;

        buffer.move_to_start();

        Ok(Document {
            id,
            buffer,
            options: DocumentOptions {
                line_ending,
                ..DocumentOptions::default()
            },
            file_path: Some(path.to_path_buf()),
            revision: 0,
            last_saved_revision: 0,
            is_read_only: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
        })
    }

    pub fn set_syntax(&mut self, syntax: Syntax) {
        self.syntax = Some(syntax);
        // Initial parse
        if let Some(s) = &mut self.syntax {
            s.parse(&self.buffer);
        }
    }

    // --- Mutation Wrappers ---

    fn get_point(&self, byte_offset: usize) -> Point {
        let line = self.buffer.line_index.get_line_at(byte_offset);
        let line_start = self.buffer.line_index.get_start(line).unwrap_or(0);
        let col = byte_offset.saturating_sub(line_start);
        Point {
            row: line,
            column: col,
        }
    }

    pub fn insert_char(&mut self, ch: char) -> Result<(), RiftError> {
        let start_byte = self.buffer.cursor();
        let start_position = self.get_point(start_byte);
        let history_pos = self.byte_to_position(start_byte);

        self.buffer.insert_char(ch)?;
        self.mark_dirty();

        // Record to undo history
        let mut text = String::new();
        text.push(ch);
        self.record_edit(
            EditOperation::Insert {
                position: history_pos,
                text: text.clone(),
                len: ch.len_utf8(),
            },
            &format!("Insert '{}'", if ch == '\n' { "\\n" } else { &text }),
        );

        let added_bytes = ch.len_utf8();
        let new_end_byte = start_byte + added_bytes;
        let new_end_position = self.get_point(new_end_byte);

        // For insertion, old_end_byte == start_byte (length 0)
        let edit = InputEdit {
            start_byte,
            old_end_byte: start_byte,
            new_end_byte,
            start_position,
            old_end_position: start_position,
            new_end_position,
        };

        if let Some(syntax) = &mut self.syntax {
            syntax.update(edit, &self.buffer);
        }

        Ok(())
    }

    pub fn insert_str(&mut self, s: &str) -> Result<(), RiftError> {
        let start_byte = self.buffer.cursor();
        let start_position = self.get_point(start_byte);
        let history_pos = self.byte_to_position(start_byte);

        self.buffer.insert_str(s)?;
        self.mark_dirty();

        // Record to undo history
        if !s.is_empty() {
            self.record_edit(
                EditOperation::Insert {
                    position: history_pos,
                    text: s.to_string(),
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
            syntax.update(edit, &self.buffer);
        }
        Ok(())
    }

    pub fn delete_backward(&mut self) -> bool {
        let end_byte = self.buffer.cursor();
        if end_byte == 0 {
            return false;
        }

        // Calculate what we are about to delete
        let mut char_len = 1;
        while (self.buffer.byte_at(end_byte - char_len) & 0b11000000) == 0b10000000 {
            char_len += 1;
            if end_byte < char_len {
                break;
            }
        }

        let start_byte = end_byte - char_len;

        // Capture deleted text before deletion
        let deleted_text: String = self
            .buffer
            .line_index
            .bytes_range(start_byte..end_byte)
            .iter()
            .map(|&b| b as char)
            .collect();
        let history_start = self.byte_to_position(start_byte);
        let history_end = self.byte_to_position(end_byte);

        let old_end_position = self.get_point(end_byte);
        let start_position = self.get_point(start_byte);

        if self.buffer.delete_backward() {
            self.mark_dirty();

            // Record to undo history
            self.record_edit(
                EditOperation::Delete {
                    range: Range::new(history_start, history_end),
                    deleted_text: deleted_text.clone(),
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
                start_byte,
                old_end_byte: end_byte,
                new_end_byte: start_byte,
                start_position,
                old_end_position,
                new_end_position: start_position,
            };

            if let Some(syntax) = &mut self.syntax {
                syntax.update(edit, &self.buffer);
            }
            return true;
        }
        false
    }

    pub fn delete_forward(&mut self) -> bool {
        let start_byte = self.buffer.cursor();
        if start_byte >= self.buffer.len() {
            return false;
        }

        // Find end of char
        let mut end_byte = start_byte + 1;
        let len = self.buffer.len();
        while end_byte < len {
            let byte = self.buffer.byte_at(end_byte);
            if (byte & 0b11000000) != 0b10000000 {
                break;
            }
            end_byte += 1;
        }

        // Capture deleted text before deletion
        let deleted_text: String = self
            .buffer
            .line_index
            .bytes_range(start_byte..end_byte)
            .iter()
            .map(|&b| b as char)
            .collect();
        let history_start = self.byte_to_position(start_byte);
        let history_end = self.byte_to_position(end_byte);

        let start_position = self.get_point(start_byte);
        let old_end_position = self.get_point(end_byte);

        if self.buffer.delete_forward() {
            self.mark_dirty();

            // Record to undo history
            self.record_edit(
                EditOperation::Delete {
                    range: Range::new(history_start, history_end),
                    deleted_text: deleted_text.clone(),
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
                start_byte,
                old_end_byte: end_byte,
                new_end_byte: start_byte,
                start_position,
                old_end_position,
                new_end_position: start_position,
            };

            if let Some(syntax) = &mut self.syntax {
                syntax.update(edit, &self.buffer);
            }
            return true;
        }
        false
    }

    /// Save document to its current path
    pub fn save(&mut self) -> Result<(), RiftError> {
        let path = self
            .file_path
            .as_ref()
            .ok_or_else(|| RiftError::new(ErrorType::Io, "NO_PATH", "No file path"))?;

        self.write_to_file(path)?;
        self.last_saved_revision = self.revision;
        Ok(())
    }

    /// Save document to a new path
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), RiftError> {
        let path = path.as_ref();
        self.write_to_file(path)?;
        self.file_path = Some(path.to_path_buf());
        self.last_saved_revision = self.revision;
        Ok(())
    }

    /// Reload document from disk
    pub fn reload_from_disk(&mut self) -> Result<(), RiftError> {
        let path = self
            .file_path
            .clone()
            .ok_or_else(|| RiftError::new(ErrorType::Io, "NO_PATH", "No file path"))?;

        *self = Self::from_file(self.id, path)?;
        Ok(())
    }

    /// Mark document as dirty (increment revision)
    pub fn mark_dirty(&mut self) {
        self.revision += 1;
    }

    /// Check if document has unsaved changes
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.revision != self.last_saved_revision
    }

    /// Check if document is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Check if document has an associated file path
    #[must_use]
    pub fn has_path(&self) -> bool {
        self.file_path.is_some()
    }

    /// Set the file path
    pub fn set_path(&mut self, path: impl AsRef<Path>) {
        self.file_path = Some(path.as_ref().to_path_buf());
    }

    /// Get display name for UI (filename or "[No Name]")
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("[No Name]")
    }

    /// Get the file path if it exists
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Atomic write to file
    fn write_to_file(&self, path: &Path) -> Result<(), RiftError> {
        use std::fs;

        // Get buffer contents
        let before = self.buffer.get_before_gap();
        let after = self.buffer.get_after_gap();

        // Write atomically using a temporary file
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let temp_path = parent.join(format!(
            ".{}.tmp",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));

        // Write to temp file
        {
            let mut file = fs::File::create(&temp_path)?;
            use std::io::Write;

            let line_ending_bytes = self.options.line_ending.as_bytes();

            if self.options.line_ending == LineEnding::LF {
                // Optimized write for LF
                file.write_all(&before)?;
                file.write_all(&after)?;
            } else {
                // Denormalize for CRLF
                Self::write_denormalized(&mut file, &before, line_ending_bytes)?;
                Self::write_denormalized(&mut file, &after, line_ending_bytes)?;
            }
            file.sync_all()?;
        }

        // Atomically rename
        fs::rename(&temp_path, path)?;

        Ok(())
    }

    /// Helper to write bytes with denormalized line endings
    fn write_denormalized(
        mut writer: impl io::Write,
        bytes: &[u8],
        line_ending: &[u8],
    ) -> io::Result<()> {
        let mut start = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            if byte == b'\n' {
                writer.write_all(&bytes[start..i])?;
                writer.write_all(line_ending)?;
                start = i + 1;
            }
        }
        if start < bytes.len() {
            writer.write_all(&bytes[start..])?;
        }
        Ok(())
    }

    // ==========================================================================
    // Undo/Redo Support
    // ==========================================================================

    /// Convert byte offset to history Position
    fn byte_to_position(&self, byte_offset: usize) -> Position {
        let line = self.buffer.line_index.get_line_at(byte_offset);
        let line_start = self.buffer.line_index.get_start(line).unwrap_or(0);
        let col = byte_offset.saturating_sub(line_start);
        Position::new(line as u32, col as u32)
    }

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

    /// Record an edit operation (either to current transaction or immediately)
    fn record_edit(&mut self, op: EditOperation, description: &str) {
        if let Some(ref mut tx) = self.current_transaction {
            tx.record(op);
        } else {
            // Auto-commit single operation
            let mut tx = EditTransaction::new(description);
            tx.record(op);
            self.history.push(tx, None);
        }
    }

    /// Undo the last edit
    /// Returns true if undo was successful
    pub fn undo(&mut self) -> bool {
        if !self.history.can_undo() {
            return false;
        }

        // Get the transaction to undo
        let inverse_ops = if let Some(tx) = self.history.current_transaction() {
            tx.inverse()
        } else {
            return false;
        };

        // Apply inverse operations
        for op in inverse_ops {
            self.apply_operation(&op);
        }

        // Move in the tree
        self.history.undo();
        self.mark_dirty();

        // Force full reparse (incremental parse would have stale positions)
        if let Some(syntax) = &mut self.syntax {
            syntax.reparse(&self.buffer);
        }

        true
    }

    /// Redo the last undone edit
    /// Returns true if redo was successful
    pub fn redo(&mut self) -> bool {
        if !self.history.can_redo() {
            return false;
        }

        // Move in the tree first to get the transaction
        if self.history.redo().is_none() {
            return false;
        }

        // Get operations to redo
        let ops = if let Some(tx) = self.history.current_transaction() {
            tx.ops.clone()
        } else {
            return false;
        };

        // Apply operations
        for op in ops {
            self.apply_operation(&op);
        }

        self.mark_dirty();

        // Force full reparse (incremental parse would have stale positions)
        if let Some(syntax) = &mut self.syntax {
            syntax.reparse(&self.buffer);
        }

        true
    }

    /// Apply an edit operation to the buffer (for undo/redo)
    pub(crate) fn apply_operation(&mut self, op: &EditOperation) {
        match op {
            EditOperation::Insert { position, text, .. } => {
                // Convert position to byte offset
                let line_start = self
                    .buffer
                    .line_index
                    .get_start(position.line as usize)
                    .unwrap_or(0);
                let byte_offset = line_start + position.col as usize;
                let _ = self.buffer.set_cursor(byte_offset);
                let _ = self.buffer.insert_str(text);
            }
            EditOperation::Delete { range, .. } => {
                // Convert range to byte offsets
                let start_line_start = self
                    .buffer
                    .line_index
                    .get_start(range.start.line as usize)
                    .unwrap_or(0);
                let start_offset = start_line_start + range.start.col as usize;
                let end_line_start = self
                    .buffer
                    .line_index
                    .get_start(range.end.line as usize)
                    .unwrap_or(0);
                let end_offset = end_line_start + range.end.col as usize;

                // Position cursor at start and delete
                let _ = self.buffer.set_cursor(start_offset);
                for _ in start_offset..end_offset {
                    self.buffer.delete_forward();
                }
            }
            EditOperation::Replace {
                range, new_text, ..
            } => {
                // Delete old content
                let start_line_start = self
                    .buffer
                    .line_index
                    .get_start(range.start.line as usize)
                    .unwrap_or(0);
                let start_offset = start_line_start + range.start.col as usize;
                let end_line_start = self
                    .buffer
                    .line_index
                    .get_start(range.end.line as usize)
                    .unwrap_or(0);
                let end_offset = end_line_start + range.end.col as usize;

                let _ = self.buffer.set_cursor(start_offset);
                for _ in start_offset..end_offset {
                    self.buffer.delete_forward();
                }
                // Insert new content
                let _ = self.buffer.insert_str(new_text);
            }
            EditOperation::BlockChange {
                range, new_content, ..
            } => {
                // For block changes, we need to replace line by line
                let start_line_start = self
                    .buffer
                    .line_index
                    .get_start(range.start.line as usize)
                    .unwrap_or(0);
                let start_offset = start_line_start + range.start.col as usize;
                let end_line_start = self
                    .buffer
                    .line_index
                    .get_start(range.end.line as usize)
                    .unwrap_or(0);
                let end_offset = end_line_start + range.end.col as usize;

                let _ = self.buffer.set_cursor(start_offset);
                for _ in start_offset..end_offset {
                    self.buffer.delete_forward();
                }
                // Insert new content
                let new_text = new_content.join("\n");
                let _ = self.buffer.insert_str(&new_text);
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
        use crate::history::DocumentSnapshot;

        // Build snapshot from buffer
        let before = self.buffer.get_before_gap();
        let after = self.buffer.get_after_gap();
        let mut full_text = String::with_capacity(before.len() + after.len());

        // Convert bytes to string (handling invalid UTF-8 gracefully)
        if let Ok(s) = std::str::from_utf8(&before) {
            full_text.push_str(s);
        }
        if let Ok(s) = std::str::from_utf8(&after) {
            full_text.push_str(s);
        }

        let snapshot = DocumentSnapshot::new(full_text);
        self.history.checkpoint(snapshot);
    }

    /// Navigate to a specific edit sequence in the undo tree
    /// Returns true if successful
    pub fn goto_seq(&mut self, target: u64) -> Result<(), crate::history::UndoError> {
        let replay_path = self.history.goto_seq(target)?;

        // Apply undo operations (inverse in reverse order)
        for tx in &replay_path.undo_ops {
            for op in tx.inverse() {
                self.apply_operation(&op);
            }
        }

        // Apply redo operations (forward in order)
        for tx in &replay_path.redo_ops {
            for op in &tx.ops {
                self.apply_operation(op);
            }
        }

        self.mark_dirty();

        // Force full reparse
        if let Some(syntax) = &mut self.syntax {
            syntax.reparse(&self.buffer);
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
