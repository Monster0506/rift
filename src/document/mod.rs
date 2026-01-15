//! Document management
//! Encapsulates buffer + file metadata for multi-buffer support

use crate::buffer::TextBuffer;
use crate::error::{ErrorType, RiftError};
use crate::history::{EditOperation, EditTransaction, Position, Range, UndoTree};
use crate::search::{find_next, SearchDirection};
use crate::syntax::Syntax;
use std::io;
use std::path::{Path, PathBuf};
use tree_sitter::{InputEdit, Point};

pub mod definitions;
pub mod manager;
use definitions::DocumentOptions;
pub use manager::DocumentManager;

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
        let mut text = Vec::new();
        text.push(crate::character::Character::from(ch));
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

        // Record to undo history
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

        // Capture deleted text before deletion
        // We delete one Character
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

            // Record to undo history
            let deleted_text_vec = vec![deleted_char];
            self.record_edit(
                EditOperation::Delete {
                    range: Range::new(history_start, history_end),
                    deleted_text: deleted_text_vec,
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

            // Calculate byte offsets for Tree-sitter
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

            // Record to undo history
            let deleted_text_vec = vec![deleted_char];
            self.record_edit(
                EditOperation::Delete {
                    range: Range::new(history_start, history_end),
                    deleted_text: deleted_text_vec,
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

    /// Delete a range of characters
    /// This method integrates with the undo system
    pub fn delete_range(&mut self, start: usize, end: usize) -> Result<(), RiftError> {
        if start >= end {
            return Ok(()); // Nothing to delete
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

        // Capture deleted text before deletion
        use crate::buffer::api::BufferView;
        let deleted_chars: Vec<crate::character::Character> =
            self.buffer.chars(start..end).collect();

        let history_start = self.byte_to_position(start);
        let history_end = self.byte_to_position(end);

        let start_position = self.get_point(start);
        let old_end_position = self.get_point(end);

        // Position cursor at start and delete characters one by one
        self.buffer.set_cursor(start)?;
        let count = end - start;
        for _ in 0..count {
            if !self.buffer.delete_forward() {
                break;
            }
        }

        self.mark_dirty();

        // Record to undo history
        self.record_edit(
            EditOperation::Delete {
                range: Range::new(history_start, history_end),
                deleted_text: deleted_chars,
            },
            &format!("Delete {} chars", count),
        );

        // Calculate byte offsets for Tree-sitter
        let start_byte = self.buffer.char_to_byte(start);
        let old_end_byte = self.buffer.char_to_byte(start + count);

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

    /// Save document to its current path
    pub fn save(&mut self) -> Result<(), RiftError> {
        let path = self.file_path.as_ref().ok_or_else(|| {
            RiftError::new(
                ErrorType::Io,
                crate::constants::errors::NO_PATH,
                "No file path",
            )
        })?;

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
        let path = self.file_path.clone().ok_or_else(|| {
            RiftError::new(
                ErrorType::Io,
                crate::constants::errors::NO_PATH,
                "No file path",
            )
        })?;

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

    /// Get the current revision number
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
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
            .unwrap_or(crate::constants::ui::NO_NAME)
    }

    /// Get the file path if it exists
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Atomic write to file
    fn write_to_file(&self, path: &Path) -> Result<(), RiftError> {
        use std::fs;

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

            // Loop over lines and write each
            for i in 0..self.buffer.get_total_lines() {
                let line_bytes = self.buffer.get_line_bytes(i);

                // Write the line content
                file.write_all(&line_bytes)?;

                // If not the last line, write the line ending
                if i < self.buffer.get_total_lines() - 1 {
                    file.write_all(line_ending_bytes)?;
                }
            }

            file.sync_all()?;
        }

        // Atomically rename
        fs::rename(&temp_path, path)?;

        Ok(())
    }

    // ==========================================================================
    // Undo/Redo Support
    // ==========================================================================

    /// Convert character index to history Position
    fn byte_to_position(&self, char_idx: usize) -> Position {
        // Method name is byte_to_position but we use char_idx from Buffer
        let line = self.buffer.line_index.get_line_at(char_idx);
        let line_start = self.buffer.line_index.get_start(line).unwrap_or(0);
        let col = char_idx.saturating_sub(line_start);
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
            // Invalidate tree so next job forces full parse
            syntax.tree = None;
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
            syntax.tree = None;
        }

        true
    }

    /// Apply an edit operation to the buffer (for undo/redo)
    pub(crate) fn apply_operation(&mut self, op: &EditOperation) {
        Self::apply_operation_to_buffer(&mut self.buffer, op);
    }

    /// Apply an edit operation to a buffer
    fn apply_operation_to_buffer(buffer: &mut TextBuffer, op: &EditOperation) {
        match op {
            EditOperation::Insert { position, text, .. } => {
                // Convert position to char offset
                let line_start = buffer
                    .line_index
                    .get_start(position.line as usize)
                    .unwrap_or(0);
                let char_offset = line_start + position.col as usize;
                let _ = buffer.set_cursor(char_offset);
                let _ = buffer.insert_chars(&text);
            }
            EditOperation::Delete { range, .. } => {
                // Convert range to char offsets
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

                // Position cursor at start and delete
                let _ = buffer.set_cursor(start_offset);
                // Difference in chars
                let count = end_offset.saturating_sub(start_offset);
                for _ in 0..count {
                    buffer.delete_forward();
                }
            }
            EditOperation::Replace {
                range, new_text, ..
            } => {
                // Delete old content
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

                let _ = buffer.set_cursor(start_offset);
                let count = end_offset.saturating_sub(start_offset);
                for _ in 0..count {
                    buffer.delete_forward();
                }
                // Insert new content
                let _ = buffer.insert_chars(new_text);
            }
            EditOperation::BlockChange {
                range, new_content, ..
            } => {
                // For block changes, we need to replace line by line
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

                let _ = buffer.set_cursor(start_offset);
                let count = end_offset.saturating_sub(start_offset);
                for _ in 0..count {
                    buffer.delete_forward();
                }
                // Insert new content
                for (i, line) in new_content.iter().enumerate() {
                    if i > 0 {
                        let _ = buffer.insert_character(crate::character::Character::Newline);
                    }
                    let _ = buffer.insert_chars(line);
                }
            }
        }
    }

    /// Apply a replay path to a buffer
    fn apply_replay_path_to_buffer(buffer: &mut TextBuffer, path: &crate::history::ReplayPath) {
        // Apply undo operations (inverse in reverse order)
        for tx in &path.undo_ops {
            for op in tx.inverse() {
                Self::apply_operation_to_buffer(buffer, &op);
            }
        }

        // Apply redo operations (forward in order)
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

        // Build snapshot from buffer characters
        let full_text: Vec<crate::character::Character> =
            self.buffer.chars(0..self.buffer.len()).collect();

        let snapshot = DocumentSnapshot::new(full_text);
        self.history.checkpoint(snapshot);
    }

    /// Navigate to a specific edit sequence in the undo tree
    /// Returns true if successful
    pub fn goto_seq(&mut self, target: u64) -> Result<(), crate::history::UndoError> {
        let replay_path = self.history.goto_seq(target)?;

        Self::apply_replay_path_to_buffer(&mut self.buffer, &replay_path);

        self.mark_dirty();

        // Force full reparse
        if let Some(syntax) = &mut self.syntax {
            syntax.tree = None;
        }

        Ok(())
    }

    /// Perform a search in the document
    /// Returns the match if found, along with timing statistics
    pub fn perform_search(
        &self,
        query: &str,
        direction: SearchDirection,
        skip_current: bool,
    ) -> Result<
        (
            Option<crate::search::SearchMatch>,
            crate::search::SearchStats,
        ),
        RiftError,
    > {
        let mut cursor = self.buffer.cursor();

        // If searching forward and skipping current, advance cursor to avoid
        // matching at current position
        if skip_current && direction == SearchDirection::Forward {
            cursor = cursor.saturating_add(1);
        }

        match find_next(&self.buffer, cursor, query, direction) {
            Ok((m, stats)) => Ok((m, stats)),
            Err(e) => Err(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::SEARCH_ERROR,
                e.to_string(),
            )),
        }
    }

    /// Find all occurrences of the pattern in the document
    pub fn find_all_matches(
        &self,
        query: &str,
    ) -> Result<(Vec<crate::search::SearchMatch>, crate::search::SearchStats), RiftError> {
        crate::search::find_all(&self.buffer, query)
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

    /// Mark the document as saved at a specific revision
    pub fn mark_as_saved(&mut self, revision: u64) {
        self.last_saved_revision = revision;
    }

    /// Apply content loaded from a background job
    pub fn apply_loaded_content(
        &mut self,
        line_index: crate::buffer::line_index::LineIndex,
        line_ending: LineEnding,
    ) {
        // Create new text buffer wrapping the loaded line index
        // We reuse the current capacity logic or just new
        let mut buffer = TextBuffer::new(4096).unwrap_or_else(|_| {
            // Should not happen as capacity is just recommendation
            // and we are replacing line_index anyway.
            panic!("Failed to create buffer")
        });

        buffer.line_index = line_index;
        // Construct the buffer revision as 0 for new file
        buffer.revision = 0;

        self.buffer = buffer;
        self.options.line_ending = line_ending;
        self.revision = 0;
        self.last_saved_revision = 0;
        self.history = UndoTree::new();
        self.current_transaction = None;
        self.syntax = None; // Needs re-parsing
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
