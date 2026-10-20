//! One seam for "read/write/list paths": every call site goes through
//! `backend()` instead of `std::fs` directly, one selector.

use crate::error::RiftError;
use std::path::{Path, PathBuf};

mod native;

/// One entry returned by [`FileSystem::list_children`].
pub struct FsEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Everything the editor needs from "a filesystem" -- only `std::fs` today,
/// but the seam means an alternate backend can slot in without touching callers.
pub trait FileSystem: Send + Sync {
    /// Read the full contents of a file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, RiftError>;

    /// Read at most `max_bytes` from the start of a file (e.g. for a
    /// preview), without reading or holding the whole thing in memory.
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened or read.
    fn read_file_prefix(&self, path: &Path, max_bytes: usize) -> Result<Vec<u8>, RiftError>;

    /// Write `content`, replacing any existing file at `path`.
    ///
    /// # Errors
    /// Returns an error if the file cannot be written.
    fn write_file(&self, path: &Path, content: &[u8]) -> Result<(), RiftError>;

    /// Whether `path` refers to anything at all (file or directory).
    fn path_exists(&self, path: &Path) -> bool;

    /// Whether `path` is a directory.
    fn is_dir(&self, path: &Path) -> bool;

    /// Whether `path`'s parent is missing (so a save/create there would fail).
    fn parent_dir_missing(&self, path: &Path) -> bool;

    /// Best-effort absolute/normalized form of `path`, for comparing two
    /// paths (e.g. "is this rename moving a directory inside itself").
    fn canonicalize(&self, path: &Path) -> PathBuf;

    /// The immediate children of a directory.
    ///
    /// # Errors
    /// Returns an error if `dir` cannot be read.
    fn list_children(&self, dir: &Path) -> Result<Vec<FsEntry>, RiftError>;

    /// Create a directory (and any missing parents).
    ///
    /// # Errors
    /// Returns an error if the directory cannot be created.
    fn create_dir(&self, path: &Path) -> Result<(), RiftError>;

    /// Create an empty file (and any missing parent directories).
    ///
    /// # Errors
    /// Returns an error if the file or its parents cannot be created.
    fn create_file(&self, path: &Path) -> Result<(), RiftError>;

    /// Move/rename a file or directory (recursive for directories).
    ///
    /// # Errors
    /// Returns an error if the source cannot be moved to `new`.
    fn rename(&self, old: &Path, new: &Path) -> Result<(), RiftError>;

    /// Recursively delete a directory, or a single file if `path` isn't one.
    ///
    /// # Errors
    /// Returns an error if the path cannot be removed.
    fn delete_recursive(&self, path: &Path) -> Result<(), RiftError>;
}

#[must_use]
pub fn backend() -> &'static dyn FileSystem {
    &native::NativeFileSystem
}

#[cfg(test)]
mod tests;
