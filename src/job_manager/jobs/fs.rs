use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// Job to copy a file or directory (recursively)
#[derive(Debug)]
pub struct FsCopyJob {
    source: PathBuf,
    destination: PathBuf,
}

impl FsCopyJob {
    pub fn new(source: PathBuf, destination: PathBuf) -> Self {
        Self {
            source,
            destination,
        }
    }

    pub fn copy_recursive_pub(source: &Path, destination: &Path) -> std::io::Result<()> {
        if source.is_dir() && destination.starts_with(source) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "cannot copy {:?}: destination {:?} is inside the source",
                    source, destination
                ),
            ));
        }
        if source.is_dir() {
            fs::create_dir_all(destination)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                let dest_child = destination.join(entry.file_name());
                if entry.file_type()?.is_dir() {
                    Self::copy_recursive_pub(&entry.path(), &dest_child)?;
                } else {
                    fs::copy(entry.path(), dest_child)?;
                }
            }
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, destination)?;
        }
        Ok(())
    }

    fn copy_recursive(
        source: &Path,
        destination: &Path,
        signal: &CancellationSignal,
    ) -> std::io::Result<()> {
        if signal.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Cancelled",
            ));
        }

        if source.is_dir() {
            fs::create_dir_all(destination)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    Self::copy_recursive(
                        &entry.path(),
                        &destination.join(entry.file_name()),
                        signal,
                    )?;
                } else {
                    fs::copy(entry.path(), destination.join(entry.file_name()))?;
                }
            }
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, destination)?;
        }
        Ok(())
    }
}

impl Job for FsCopyJob {
    fn name(&self) -> &'static str {
        "fs-copy"
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        let _ = sender.send(JobMessage::Progress(
            id,
            0,
            format!("Copying {:?} to {:?}", self.source, self.destination),
        ));

        match Self::copy_recursive(&self.source, &self.destination, &signal) {
            Ok(_) => {
                let _ = sender.send(JobMessage::Finished(id, false));
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_recursive_pub_errors_when_destination_is_inside_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("mydir");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("file.txt"), "hello").unwrap();

        // destination is a subdirectory of source — must be rejected
        let destination = source.join("sub");
        let result = FsCopyJob::copy_recursive_pub(&source, &destination);
        assert!(
            result.is_err(),
            "copy_recursive_pub must error when destination is inside source"
        );
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn copy_recursive_pub_copies_file_successfully() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("hello.txt");
        std::fs::write(&src, "world").unwrap();
        let dst = dir.path().join("hello_copy.txt");
        FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "world");
    }

    #[test]
    fn copy_recursive_pub_copies_directory_successfully() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("srcdir");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "aaa").unwrap();
        let dst = dir.path().join("dstdir");
        FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "aaa");
    }

    #[test]
    fn copy_recursive_pub_copies_nested_directory() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("outer");
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("inner").join("deep.txt"), "deep").unwrap();
        let dst = dir.path().join("copy");
        FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
        assert_eq!(
            std::fs::read_to_string(dst.join("inner").join("deep.txt")).unwrap(),
            "deep"
        );
    }

    #[test]
    fn copy_recursive_pub_copies_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("empty");
        std::fs::create_dir(&src).unwrap();
        let dst = dir.path().join("empty_copy");
        FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
        assert!(
            dst.is_dir(),
            "empty directory must be created at destination"
        );
    }

    #[test]
    fn copy_recursive_pub_errors_on_nonexistent_source() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("does_not_exist");
        let dst = dir.path().join("dst");
        let result = FsCopyJob::copy_recursive_pub(&src, &dst);
        // Source doesn't exist: is_dir() returns false, falls into the file branch,
        // then fs::copy fails because the source path doesn't exist.
        assert!(result.is_err(), "missing source must return an error");
    }

    #[test]
    fn copy_recursive_pub_file_to_existing_destination_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        std::fs::write(&src, "new content").unwrap();
        std::fs::write(&dst, "old content").unwrap();
        FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "new content");
    }

    #[test]
    fn copy_recursive_pub_preserves_file_contents_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("binary.bin");
        let data: Vec<u8> = (0u8..=255).collect();
        std::fs::write(&src, &data).unwrap();
        let dst = dir.path().join("binary_copy.bin");
        FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), data);
    }

    #[test]
    fn copy_recursive_pub_destination_prefix_not_confused_with_inside_source() {
        // "src_extra" starts with "src" but is NOT inside "src" — must not be rejected.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("f.txt"), "hi").unwrap();
        let dst = dir.path().join("src_extra");
        FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("f.txt")).unwrap(), "hi");
    }
}

/// Job to move/rename a file or directory
#[derive(Debug)]
pub struct FsMoveJob {
    source: PathBuf,
    destination: PathBuf,
}

impl FsMoveJob {
    pub fn new(source: PathBuf, destination: PathBuf) -> Self {
        Self {
            source,
            destination,
        }
    }
}

impl Job for FsMoveJob {
    fn name(&self) -> &'static str {
        "fs-move"
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        let _ = sender.send(JobMessage::Progress(
            id,
            0,
            format!("Moving {:?} to {:?}", self.source, self.destination),
        ));

        if let Some(parent) = self.destination.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
                return;
            }
        }

        match fs::rename(&self.source, &self.destination) {
            Ok(_) => {
                let _ = sender.send(JobMessage::Finished(id, false));
            }
            Err(_) => match FsCopyJob::copy_recursive_pub(&self.source, &self.destination) {
                Ok(_) => {
                    let del = if self.source.is_dir() {
                        fs::remove_dir_all(&self.source)
                    } else {
                        fs::remove_file(&self.source)
                    };
                    match del {
                        Ok(_) => {
                            let _ = sender.send(JobMessage::Finished(id, false));
                        }
                        Err(e) => {
                            let _ = sender.send(JobMessage::Error(id, e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    let _ = sender.send(JobMessage::Error(id, e.to_string()));
                }
            },
        }
    }
}

/// Job to delete a file or directory
#[derive(Debug)]
pub struct FsDeleteJob {
    path: PathBuf,
}

impl FsDeleteJob {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Job for FsDeleteJob {
    fn name(&self) -> &'static str {
        "fs-delete"
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        let _ = sender.send(JobMessage::Progress(
            id,
            0,
            format!("Deleting {:?}", self.path),
        ));

        let result = if self.path.is_dir() {
            fs::remove_dir_all(&self.path)
        } else {
            fs::remove_file(&self.path)
        };

        match result {
            Ok(_) => {
                let _ = sender.send(JobMessage::Finished(id, false));
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
            }
        }
    }
}

/// Job to create a file or directory
#[derive(Debug)]
pub struct FsCreateJob {
    path: PathBuf,
    is_dir: bool,
}

impl FsCreateJob {
    pub fn new(path: PathBuf, is_dir: bool) -> Self {
        Self { path, is_dir }
    }
}

impl Job for FsCreateJob {
    fn name(&self) -> &'static str {
        "fs-create"
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        let result = if self.is_dir {
            fs::create_dir_all(&self.path)
        } else {
            if let Some(parent) = self.path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    let _ = sender.send(JobMessage::Error(id, e.to_string()));
                    return;
                }
            }
            fs::File::create(&self.path).map(|_| ())
        };

        match result {
            Ok(_) => {
                let _ = sender.send(JobMessage::Finished(id, false));
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
            }
        }
    }
}

/// Job to delete multiple files or directories
#[derive(Debug)]
pub struct FsBatchDeleteJob {
    paths: Vec<PathBuf>,
}

impl FsBatchDeleteJob {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }
}

impl Job for FsBatchDeleteJob {
    fn name(&self) -> &'static str {
        "fs-batch-delete"
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        let total = self.paths.len();
        for (i, path) in self.paths.iter().enumerate() {
            if signal.is_cancelled() {
                return;
            }

            let _ = sender.send(JobMessage::Progress(
                id,
                ((i as f32 / total as f32) * 100.0) as u32,
                format!(
                    "Deleting {:?} ({}/{})",
                    path.file_name().unwrap_or_default(),
                    i + 1,
                    total
                ),
            ));

            let result = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };

            if let Err(e) = result {
                let _ = sender.send(JobMessage::Error(
                    id,
                    format!("Failed to delete {:?}: {}", path, e),
                ));
            }
        }

        let _ = sender.send(JobMessage::Finished(id, false));
    }
}
