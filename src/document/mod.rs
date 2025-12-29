//! Document management
//! Encapsulates buffer + file metadata for multi-buffer support

use crate::buffer::TextBuffer;
use crate::error::{ErrorType, RiftError};
use std::io;
use std::path::{Path, PathBuf};

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
            options: DocumentOptions { line_ending },
            file_path: Some(path.to_path_buf()),
            revision: 0,
            last_saved_revision: 0,
            is_read_only: false,
        })
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
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
