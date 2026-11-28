//! Document persistence: save, load, path management, display name.

use super::Document;
#[cfg(test)]
use crate::error::{ErrorType, RiftError};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

impl Document {
    #[cfg(test)]
    /// Save document to its current path
    ///
    /// # Errors
    /// Returns an error if the document has no file path or the write fails.
    pub(super) fn save(&mut self) -> Result<(), RiftError> {
        let path = self.file_path.as_ref().ok_or_else(|| {
            RiftError::new(
                ErrorType::Io,
                crate::constants::errors::NO_PATH,
                "No file path",
            )
        })?;
        crate::perf_span!("document_save", crate::perf::PerfFields::default());
        self.write_to_file(path)?;
        self.history.mark_saved();
        Ok(())
    }

    #[cfg(test)]
    /// Save document to a new path
    ///
    /// # Errors
    /// Returns an error if the file cannot be written.
    fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), RiftError> {
        let path = path.as_ref();
        crate::perf_span!("document_save", crate::perf::PerfFields::default());
        self.write_to_file(path)?;
        self.file_path = Some(path.to_path_buf());
        self.history.mark_saved();
        Ok(())
    }

    /// Check if document has unsaved changes
    #[must_use]
    pub(crate) const fn is_dirty(&self) -> bool {
        !self.history.is_at_saved()
    }

    /// Set the file path (normalized to absolute path for consistent comparison)
    pub(crate) fn set_path(&mut self, path: impl AsRef<Path>) {
        self.file_path = Some(Self::normalize_path(path.as_ref()));
    }

    /// Normalize a path to an absolute path.
    pub(super) fn normalize_path(path: &Path) -> PathBuf {
        crate::fs_backend::backend().canonicalize(path)
    }

    /// Get display name for UI (filename or "[No Name]")
    #[must_use]
    pub(crate) fn display_name(&self) -> Cow<'_, str> {
        if let Some(title) = self.scratch_title() {
            return Cow::Borrowed(title);
        }
        if let Some(path) = self.directory_path() {
            return path
                .file_name()
                .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy());
        }
        if let Some(label) = self.git_output_label() {
            return Cow::Owned(label);
        }
        if let Some(path) = self.git_blame_path() {
            let name = path
                .file_name()
                .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy());
            return Cow::Owned(format!("[Git Blame] {name}"));
        }

        if let Some(crate::annotations::value::Value::Str(title)) = self.vars.get("title") {
            return Cow::Borrowed(title);
        }

        self.kind.display_name(
            self.file_path.as_deref(),
            self.terminal().map(|terminal| terminal.name.as_str()),
        )
    }

    /// Get the file path if it exists
    #[must_use]
    pub(crate) fn path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    pub(crate) const fn save_view_state(&mut self, scroll: crate::viewport::ScrollPosition) {
        self.view_state = scroll;
    }

    pub(crate) const fn saved_view_state(&self) -> crate::viewport::ScrollPosition {
        self.view_state
    }

    /// Mark the document as saved at a specific edit sequence
    pub(crate) const fn mark_as_saved(&mut self, saved_seq: crate::history::EditSeq) {
        self.history.mark_saved_at(saved_seq);
    }

    #[cfg(test)]
    /// Write to file via `fs_backend`, so this goes through whichever
    /// implementation is registered for this target.
    fn write_to_file(&self, path: &Path) -> Result<(), RiftError> {
        let mut content = Vec::new();
        if self.options.has_bom() {
            content.extend_from_slice(&super::factories::UTF8_BOM);
        }
        let line_ending_bytes = self.options.line_ending().as_bytes();
        let total_lines = self.buffer.get_total_lines();
        for i in 0..total_lines {
            content.extend_from_slice(&self.buffer.get_line_bytes(i));
            if i < total_lines - 1 {
                content.extend_from_slice(line_ending_bytes);
            }
        }
        crate::fs_backend::backend().write_file(path, &content)
    }
}

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod persistence_tests;
