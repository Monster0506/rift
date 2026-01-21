use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload};
use std::any::Any;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug)]
pub struct DirectoryListing {
    pub path: PathBuf,
    pub entries: Vec<FileEntry>,
}

impl JobPayload for DirectoryListing {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[derive(Debug)]
pub struct DirectoryListJob {
    path: PathBuf,
    show_hidden: bool,
}

impl DirectoryListJob {
    pub fn new(path: PathBuf, show_hidden: bool) -> Self {
        Self { path, show_hidden }
    }
}

impl Job for DirectoryListJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        match fs::read_dir(&self.path) {
            Ok(entries) => {
                let mut file_entries = Vec::new();

                for entry in entries.flatten() {
                    if signal.is_cancelled() {
                        return;
                    }
                    let path = entry.path();
                    let file_name = entry.file_name();
                    let name_str = file_name.to_string_lossy().to_string();

                    // Filter hidden
                    if !self.show_hidden && name_str.starts_with('.') {
                        continue;
                    }
                    // TODO: Filter gitignore (requires parsing .gitignore in parent dirs)

                    let metadata = entry.metadata().ok();
                    let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

                    file_entries.push(FileEntry {
                        path,
                        name: name_str,
                        is_dir,
                        size,
                    });
                }

                // Sort: Directories first, then alphabetical
                file_entries.sort_by(|a, b| {
                    if a.is_dir != b.is_dir {
                        return b.is_dir.cmp(&a.is_dir);
                    }
                    // Case-insensitive sort
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                });

                let result = Box::new(DirectoryListing {
                    path: self.path,
                    entries: file_entries,
                });

                let _ = sender.send(JobMessage::Custom(id, result));
                let _ = sender.send(JobMessage::Finished(id, true)); // Silent finish
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub struct FilePreview {
    pub path: PathBuf,
    pub content: String,
}

impl JobPayload for FilePreview {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[derive(Debug)]
pub struct FilePreviewJob {
    path: PathBuf,
}

impl FilePreviewJob {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Job for FilePreviewJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        // Check file size first - don't preview HUGE files (e.g. > 1MB)
        if let Ok(metadata) = fs::metadata(&self.path) {
            if metadata.len() > 1024 * 1024 {
                let result = Box::new(FilePreview {
                    path: self.path,
                    content: "<File too large to preview>".to_string(),
                });
                let _ = sender.send(JobMessage::Custom(id, result));
                let _ = sender.send(JobMessage::Finished(id, true));
                return;
            }
        }

        // Try to read as text
        // For MVP/Robustness, we'll try read_to_string. If utf8 error, it's binary.
        // We only read the first chunk.

        let path = self.path.clone();

        // This is a simplified "head"
        use std::io::Read;
        let mut file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
                return;
            }
        };

        let mut buffer = [0; 4096]; // Read 4KB
        match file.read(&mut buffer) {
            Ok(n) => {
                let slice = &buffer[..n];
                // Check if valid UTF-8
                match std::str::from_utf8(slice) {
                    Ok(s) => {
                        let preview: String = s.lines().take(100).collect::<Vec<_>>().join("\n");
                        let result = Box::new(FilePreview {
                            path,
                            content: preview,
                        });
                        let _ = sender.send(JobMessage::Custom(id, result));
                        let _ = sender.send(JobMessage::Finished(id, true));
                    }
                    Err(_) => {
                        let result = Box::new(FilePreview {
                            path,
                            content: "<Binary file>".to_string(),
                        });
                        let _ = sender.send(JobMessage::Custom(id, result));
                        let _ = sender.send(JobMessage::Finished(id, true));
                    }
                }
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}
