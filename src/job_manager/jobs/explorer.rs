use crate::job_manager::{CancellationSignal, Job, JobMessage};
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
    pub doc_id: usize,
    pub path: PathBuf,
    pub entries: Vec<FileEntry>,
}

crate::impl_job_payload!(DirectoryListing);

#[derive(Debug)]
pub struct DirectoryListJob {
    doc_id: usize,
    path: PathBuf,
    show_hidden: bool,
}

impl DirectoryListJob {
    pub fn new(doc_id: usize, path: PathBuf, show_hidden: bool) -> Self {
        Self {
            doc_id,
            path,
            show_hidden,
        }
    }
}

impl Job for DirectoryListJob {
    fn name(&self) -> &'static str {
        "directory-list"
    }

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
                    doc_id: self.doc_id,
                    path: self.path,
                    entries: file_entries,
                });

                crate::job_manager::send_job_result(&sender, id, result);
            }
            Err(e) => {
                // Empty listing clears "Loading..." before the error notification
                let result = Box::new(DirectoryListing {
                    doc_id: self.doc_id,
                    path: self.path.clone(),
                    entries: vec![],
                });
                let _ = sender.send(JobMessage::Custom(id, result));
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}
