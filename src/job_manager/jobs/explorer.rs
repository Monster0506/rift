use crate::job_manager::{CancellationSignal, Job, JobMessage};
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

        // TODO: Filter gitignore (requires parsing .gitignore in parent dirs)
        match crate::fs_backend::backend().list_children(&self.path) {
            Ok(children) => {
                let mut file_entries: Vec<FileEntry> = children
                    .into_iter()
                    .filter(|c| self.show_hidden || !c.name.starts_with('.'))
                    .map(|c| FileEntry {
                        path: self.path.join(&c.name),
                        name: c.name,
                        is_dir: c.is_dir,
                        size: 0,
                    })
                    .collect();
                // Directories first, then alphabetical (case-insensitive),
                // each name lowercased exactly once for the sort key.
                file_entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));
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
                let _ = sender.send(JobMessage::Error(id, e.message));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}
