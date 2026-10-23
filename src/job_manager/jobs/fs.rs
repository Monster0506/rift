use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

#[cfg(unix)]
const CROSS_DEVICE_OS_CODE: i32 = 18;
#[cfg(windows)]
const CROSS_DEVICE_OS_CODE: i32 = 17;

#[must_use]
pub(crate) fn is_cross_device(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::CrossesDevices {
        return true;
    }

    #[cfg(any(unix, windows))]
    {
        err.raw_os_error() == Some(CROSS_DEVICE_OS_CODE)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

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
pub(crate) struct FsCopyJob {
    source: PathBuf,
    destination: PathBuf,
}

impl FsCopyJob {
    #[cfg(test)]
    #[must_use]
    pub(in crate::job_manager) const fn new(source: PathBuf, destination: PathBuf) -> Self {
        Self {
            source,
            destination,
        }
    }

    /// Recursively copy `source` into `destination`.
    ///
    /// # Errors
    /// Returns `Err` if `destination` is inside `source` or any file operation fails.
    pub(crate) fn copy_recursive_pub(source: &Path, destination: &Path) -> std::io::Result<()> {
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
                        "cannot copy {}: destination {} is inside the source",
                        source.display(),
                        destination.display()
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
            format!(
                "Copying {} to {}",
                self.source.display(),
                self.destination.display()
            ),
        ));

        let mut visited = HashSet::new();
        match Self::copy_recursive(&self.source, &self.destination, &signal, &mut visited) {
            Ok(()) => {
                let _ = sender.send(JobMessage::Finished(id, false));
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.into()));
            }
        }
    }
}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod fs_tests;

#[cfg(test)]
/// Job to move/rename a file or directory
#[derive(Debug)]
pub(in crate::job_manager) struct FsMoveJob {
    source: PathBuf,
    destination: PathBuf,
}

#[cfg(test)]
impl FsMoveJob {
    #[must_use]
    pub(in crate::job_manager) const fn new(source: PathBuf, destination: PathBuf) -> Self {
        Self {
            source,
            destination,
        }
    }
}

#[cfg(test)]
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
            format!(
                "Moving {} to {}",
                self.source.display(),
                self.destination.display()
            ),
        ));

        if let Some(parent) = self.destination.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                let _ = sender.send(JobMessage::Error(id, e.into()));
                return;
            }
        }

        let rename_err = match fs::rename(&self.source, &self.destination) {
            Ok(()) => {
                let _ = sender.send(JobMessage::Finished(id, false));
                return;
            }
            Err(e) => e,
        };

        if !is_cross_device(&rename_err) {
            let _ = sender.send(JobMessage::Error(id, rename_err.into()));
            return;
        }

        let moved = FsCopyJob::copy_recursive_pub(&self.source, &self.destination).and_then(|()| {
            if self.source.is_dir() {
                fs::remove_dir_all(&self.source)
            } else {
                fs::remove_file(&self.source)
            }
        });
        match moved {
            Ok(()) => {
                let _ = sender.send(JobMessage::Finished(id, false));
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.into()));
            }
        }
    }
}

#[cfg(test)]
/// Job to delete a file or directory
#[derive(Debug)]
pub(in crate::job_manager) struct FsDeleteJob {
    path: PathBuf,
}

#[cfg(test)]
impl FsDeleteJob {
    #[must_use]
    pub(in crate::job_manager) const fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[cfg(test)]
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
            format!("Deleting {}", self.path.display()),
        ));

        let result = if self.path.is_dir() {
            fs::remove_dir_all(&self.path)
        } else {
            fs::remove_file(&self.path)
        };

        match result {
            Ok(()) => {
                let _ = sender.send(JobMessage::Finished(id, false));
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.into()));
            }
        }
    }
}

#[cfg(test)]
/// Job to create a file or directory
#[derive(Debug)]
pub(in crate::job_manager) struct FsCreateJob {
    path: PathBuf,
    is_dir: bool,
}

#[cfg(test)]
impl FsCreateJob {
    #[must_use]
    pub(in crate::job_manager) const fn new(path: PathBuf, is_dir: bool) -> Self {
        Self { path, is_dir }
    }
}

#[cfg(test)]
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
                    let _ = sender.send(JobMessage::Error(id, e.into()));
                    return;
                }
            }
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&self.path)
                .map(|_| ())
        };

        match result {
            Ok(()) => {
                let _ = sender.send(JobMessage::Finished(id, false));
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.into()));
            }
        }
    }
}

#[cfg(test)]
/// Job to delete multiple files or directories
#[derive(Debug)]
pub(in crate::job_manager) struct FsBatchDeleteJob {
    paths: Vec<PathBuf>,
}

#[cfg(test)]
impl FsBatchDeleteJob {
    #[must_use]
    pub(in crate::job_manager) const fn new(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }
}

#[cfg(test)]
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
                u32::try_from(i * 100 / total).unwrap_or(100),
                format!(
                    "Deleting {} ({}/{})",
                    path.file_name().unwrap_or_default().to_string_lossy(),
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
                    format!("Failed to delete {}: {e}", path.display()).into(),
                ));
            }
        }

        let _ = sender.send(JobMessage::Finished(id, false));
    }
}
