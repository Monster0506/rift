//! Real `std::fs`-backed implementation of [`super::FileSystem`].

use super::{FileSystem, FsEntry};
use crate::error::RiftError;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// Resolve `.`/`..` components manually for paths that don't exist yet, then
/// anchor a still-relative result to the current directory.
fn normalize_missing(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    if out.is_absolute() {
        out
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&out))
            .unwrap_or(out)
    }
}

pub struct NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, RiftError> {
        Ok(fs::read(path)?)
    }

    fn read_file_prefix(&self, path: &Path, max_bytes: usize) -> Result<Vec<u8>, RiftError> {
        let file = fs::File::open(path)?;
        let mut buf = Vec::with_capacity(max_bytes);
        file.take(max_bytes as u64).read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn write_file(&self, path: &Path, content: &[u8]) -> Result<(), RiftError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let temp_path = parent.join(format!(
            "{}~",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));

        let write_result = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&temp_path)?;
            file.write_all(content)?;
            file.sync_all()?;
            Ok(())
        })();

        if let Err(e) = write_result {
            let _ = fs::remove_file(&temp_path);
            return Err(e.into());
        }

        fs::rename(&temp_path, path)?;
        Ok(())
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn parent_dir_missing(&self, path: &Path) -> bool {
        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => !parent.exists(),
            _ => false,
        }
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        fs::canonicalize(path).unwrap_or_else(|_| normalize_missing(path))
    }

    fn list_children(&self, dir: &Path) -> Result<Vec<FsEntry>, RiftError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // file_type() reads d_type from the readdir result itself on
            // most platforms, so listing a directory stays a single syscall.
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(FsEntry { name, is_dir });
        }
        Ok(entries)
    }

    fn create_dir(&self, path: &Path) -> Result<(), RiftError> {
        Ok(fs::create_dir_all(path)?)
    }

    fn create_file(&self, path: &Path) -> Result<(), RiftError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::File::create(path)?;
        Ok(())
    }

    fn rename(&self, old: &Path, new: &Path) -> Result<(), RiftError> {
        if let Some(parent) = new.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(old, new).or_else(|_| {
            // Cross-device fallback: copy then delete.
            crate::job_manager::jobs::fs::FsCopyJob::copy_recursive_pub(old, new).and_then(|_| {
                if old.is_dir() {
                    fs::remove_dir_all(old)
                } else {
                    fs::remove_file(old)
                }
            })
        })?;
        Ok(())
    }

    fn delete_recursive(&self, path: &Path) -> Result<(), RiftError> {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
