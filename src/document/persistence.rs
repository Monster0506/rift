//! Document persistence — save, load, path management, display name.

use super::{BufferKind, Document, LineEnding};
use crate::error::{ErrorType, RiftError};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

impl Document {
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
        self.history.mark_saved();
        Ok(())
    }

    /// Save document to a new path
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), RiftError> {
        let path = path.as_ref();
        self.write_to_file(path)?;
        self.file_path = Some(path.to_path_buf());
        self.history.mark_saved();
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

    /// No-op; dirty state is tracked via the undo tree.
    pub fn mark_dirty(&mut self) {}

    /// Check if document has unsaved changes
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.history.is_at_saved()
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

    /// Set the file path (normalized to absolute path for consistent comparison)
    pub fn set_path(&mut self, path: impl AsRef<Path>) {
        self.file_path = Some(Self::normalize_path(path.as_ref()));
    }

    /// Normalize a path to an absolute path.
    pub(super) fn normalize_path(path: &Path) -> PathBuf {
        if let Ok(canonical) = std::fs::canonicalize(path) {
            return canonical;
        }
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    }

    /// Get display name for UI (filename or "[No Name]")
    #[must_use]
    pub fn display_name(&self) -> Cow<'_, str> {
        match &self.kind {
            BufferKind::Terminal => {
                if let Some(term) = &self.terminal {
                    Cow::Owned(format!("[Terminal] {}", term.name))
                } else {
                    Cow::Borrowed("[Terminal]")
                }
            }
            BufferKind::Directory { path, .. } => Cow::Owned(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "/".to_string()),
            ),
            BufferKind::UndoTree { .. } => Cow::Borrowed("[UndoTree]"),
            BufferKind::Messages { show_all } => {
                if *show_all {
                    Cow::Borrowed("[Messages:all]")
                } else {
                    Cow::Borrowed("[Messages]")
                }
            }
            BufferKind::Clipboard { .. } => Cow::Borrowed("[Clipboard]"),
            BufferKind::ClipboardEntry {
                entry_index: Some(i),
            } => Cow::Owned(format!("[Clipboard:{}]", i)),
            BufferKind::ClipboardEntry { entry_index: None } => Cow::Borrowed("[Clipboard:new]"),
            BufferKind::File => self
                .file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(Cow::Borrowed)
                .unwrap_or(Cow::Borrowed(crate::constants::ui::NO_NAME)),
        }
    }

    /// Get the file path if it exists
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Save view state before switching away from this document.
    pub fn save_view_state(&mut self, top_line: usize, left_col: usize) {
        self.view_state.top_line = top_line;
        self.view_state.left_col = left_col;
    }

    /// Get the saved view state when switching back to this document.
    pub fn get_view_state(&self) -> &super::ViewState {
        &self.view_state
    }

    /// Apply content loaded from a background job.
    pub fn apply_loaded_content(
        &mut self,
        line_index: crate::buffer::line_index::LineIndex,
        line_ending: LineEnding,
    ) {
        use crate::buffer::TextBuffer;
        let mut buffer =
            TextBuffer::new(4096).unwrap_or_else(|_| panic!("Failed to create buffer"));
        buffer.line_index = line_index;
        buffer.revision = 0;

        self.buffer = buffer;
        self.options.line_ending = line_ending;
        self.history = crate::history::UndoTree::new();
        self.current_transaction = None;
        self.syntax = None;
    }

    /// Mark the document as saved at a specific edit sequence
    pub fn mark_as_saved(&mut self, saved_seq: crate::history::EditSeq) {
        self.history.mark_saved_at(saved_seq);
    }

    /// Atomic write to file
    fn write_to_file(&self, path: &Path) -> Result<(), RiftError> {
        use std::fs;
        use std::io::Write;

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let temp_path = parent.join(format!(
            "{}~",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));

        {
            let mut file = fs::File::create(&temp_path)?;
            let line_ending_bytes = self.options.line_ending.as_bytes();

            for i in 0..self.buffer.get_total_lines() {
                let line_bytes = self.buffer.get_line_bytes(i);
                file.write_all(&line_bytes)?;
                if i < self.buffer.get_total_lines() - 1 {
                    file.write_all(line_ending_bytes)?;
                }
            }
            file.sync_all()?;
        }

        fs::rename(&temp_path, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::document::Document;

    #[test]
    fn save_as_uses_tilde_suffix_for_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");

        let mut doc = Document::new(1).unwrap();
        doc.save_as(&path).unwrap();

        assert!(path.exists());

        let old_tmp = dir.path().join(".note.txt.tmp");
        assert!(
            !old_tmp.exists(),
            "old-style temp file should not exist: {old_tmp:?}"
        );

        let tilde_tmp = dir.path().join("note.txt~");
        assert!(
            !tilde_tmp.exists(),
            "tilde temp file should be renamed away: {tilde_tmp:?}"
        );
    }
}
