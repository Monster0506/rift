use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload};
use std::any::Any;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// Decodes a read buffer as UTF-8, dropping a trailing partial char cut off
/// by the read boundary. Only an invalid sequence earlier means real binary.
fn decode_preview_text(slice: &[u8]) -> Option<&str> {
    match std::str::from_utf8(slice) {
        Ok(s) => Some(s),
        Err(err) if err.error_len().is_none() => {
            std::str::from_utf8(&slice[..err.valid_up_to()]).ok()
        }
        Err(_) => None,
    }
}

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

                let _ = sender.send(JobMessage::Custom(id, result));
                let _ = sender.send(JobMessage::Finished(id, true)); // Silent finish
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
    fn name(&self) -> &'static str {
        "file-preview"
    }

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
                match decode_preview_text(slice) {
                    Some(s) => {
                        let preview: String = s.lines().take(100).collect::<Vec<_>>().join("\n");
                        let result = Box::new(FilePreview {
                            path,
                            content: preview,
                        });
                        let _ = sender.send(JobMessage::Custom(id, result));
                        let _ = sender.send(JobMessage::Finished(id, true));
                    }
                    None => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_manager::CancellationSignal;
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc};

    fn make_signal(cancelled: bool) -> CancellationSignal {
        CancellationSignal {
            cancelled: Arc::new(AtomicBool::new(cancelled)),
        }
    }

    #[test]
    fn test_file_preview_valid_utf8_split_at_boundary_is_not_binary() {
        // The euro sign (e2 82 ac) straddles the 4 KiB read cutoff:
        // 2 bytes land before it, 1 byte after, so a single read splits it.
        let mut content = vec![b'a'; 4096 - 2];
        content.extend_from_slice("\u{20ac}".as_bytes());
        content.extend_from_slice(b"trailing text after the split char");

        let path = std::env::temp_dir().join(format!(
            "rift_file_preview_split_utf8_{}_{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, &content).unwrap();

        let job = Box::new(FilePreviewJob::new(path.clone()));
        let (tx, rx) = mpsc::channel();
        job.run(1, tx, make_signal(false));
        let msgs: Vec<JobMessage> = rx.try_iter().collect();
        let payload = msgs
            .into_iter()
            .find_map(|m| {
                if let JobMessage::Custom(_, p) = m {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("should have Custom message");
        let result = payload.into_any().downcast::<FilePreview>().unwrap();

        let _ = fs::remove_file(&path);

        assert_ne!(
            result.content, "<Binary file>",
            "valid UTF-8 file misclassified as binary due to a multibyte char \
             straddling the read boundary"
        );
        assert!(result.content.starts_with(&"a".repeat(100)));
    }
}
