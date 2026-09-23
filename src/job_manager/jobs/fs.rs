use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// Recreate a symlink found at `entry_path` as a symlink at `dest`.
/// Falls back to copying the link's target path on platforms without symlink support.
fn recreate_symlink(entry_path: &Path, dest: &Path) -> std::io::Result<()> {
    let target = fs::read_link(entry_path)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, dest)
    }
    #[cfg(windows)]
    {
        if entry_path.is_dir() {
            std::os::windows::fs::symlink_dir(&target, dest)
        } else {
            std::os::windows::fs::symlink_file(&target, dest)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        fs::copy(entry_path, dest).map(|_| ())
    }
}

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
        let signal = CancellationSignal::new_uncancelled();
        let mut visited = HashSet::new();
        Self::copy_recursive(source, destination, &signal, &mut visited)
    }

    fn copy_recursive(
        source: &Path,
        destination: &Path,
        signal: &CancellationSignal,
        visited: &mut HashSet<PathBuf>,
    ) -> std::io::Result<()> {
        if signal.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Cancelled",
            ));
        }

        let source_meta = fs::symlink_metadata(source)?;
        if source_meta.file_type().is_symlink() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            return recreate_symlink(source, destination);
        }

        if source_meta.is_dir() {
            if destination.starts_with(source) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "cannot copy {:?}: destination {:?} is inside the source",
                        source, destination
                    ),
                ));
            }
            if let Ok(canonical) = fs::canonicalize(source) {
                if !visited.insert(canonical) {
                    return Ok(());
                }
            }

            fs::create_dir_all(destination)?;
            for entry in fs::read_dir(source)? {
                if signal.is_cancelled() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Cancelled",
                    ));
                }
                let entry = entry?;
                let dest_child = destination.join(entry.file_name());
                Self::copy_recursive(&entry.path(), &dest_child, signal, visited)?;
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

        let mut visited = HashSet::new();
        match Self::copy_recursive(&self.source, &self.destination, &signal, &mut visited) {
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
#[path = "fs_tests.rs"]
mod fs_tests;

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
