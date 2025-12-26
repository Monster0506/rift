//! Document management
//! Encapsulates buffer + file metadata for multi-buffer support

use crate::buffer::GapBuffer;
use std::io;
use std::path::{Path, PathBuf};

/// Unique identifier for documents
pub type DocumentId = u64;

/// Document combining buffer and file metadata
pub struct Document {
    /// Unique document identifier
    pub id: DocumentId,
    /// Text buffer
    pub buffer: GapBuffer,
    /// File path (None if new/unsaved)
    file_path: Option<PathBuf>,
    /// Current revision number (incremented on edits)
    revision: u64,
    /// Revision of last save
    last_saved_revision: u64,
    /// Read-only flag (for permissions or :view mode)
    pub is_read_only: bool,
}

impl Document {
    /// Create a new empty document
    pub fn new(id: DocumentId) -> Result<Self, String> {
        let buffer = GapBuffer::new(4096)?;
        Ok(Document {
            id,
            buffer,
            file_path: None,
            revision: 0,
            last_saved_revision: 0,
            is_read_only: false,
        })
    }

    /// Load document from file
    pub fn from_file(id: DocumentId, path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;

        let mut buffer = GapBuffer::new(bytes.len().max(4096))
            .map_err(io::Error::other)?;

        buffer
            .insert_bytes(&bytes)
            .map_err(io::Error::other)?;

        buffer.move_to_start();

        Ok(Document {
            id,
            buffer,
            file_path: Some(path.to_path_buf()),
            revision: 0,
            last_saved_revision: 0,
            is_read_only: false,
        })
    }

    /// Save document to its current path
    pub fn save(&mut self) -> io::Result<()> {
        let path = self
            .file_path
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No file path"))?;

        self.write_to_file(path)?;
        self.last_saved_revision = self.revision;
        Ok(())
    }

    /// Save document to a new path
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        self.write_to_file(path)?;
        self.file_path = Some(path.to_path_buf());
        self.last_saved_revision = self.revision;
        Ok(())
    }

    /// Reload document from disk
    pub fn reload_from_disk(&mut self) -> io::Result<()> {
        let path = self
            .file_path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No file path"))?;

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
    fn write_to_file(&self, path: &Path) -> io::Result<()> {
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
            file.write_all(before)?;
            file.write_all(after)?;
            file.sync_all()?;
        }

        // Atomically rename
        fs::rename(&temp_path, path)?;

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
