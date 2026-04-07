use crate::document::{DirEntry, DocumentId};
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload};
use std::any::Any;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// Payload returned by a file-explorer preview job.
#[derive(Debug)]
pub struct ExplorerPreviewResult {
    /// The document ID of the right-pane preview buffer to populate.
    pub right_doc_id: DocumentId,
    /// The path that was previewed.
    pub path: PathBuf,
    /// Directory entries if the path is a directory; `None` for file previews.
    pub dir_entries: Option<Vec<DirEntry>>,
    /// Text content if the path is a file; `None` for directory previews.
    pub file_text: Option<String>,
}

impl JobPayload for ExplorerPreviewResult {
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

/// Maximum number of bytes read for a file preview.
const FILE_PREVIEW_BYTES: usize = 8 * 1024; // 8 KiB
/// Maximum number of lines shown in a file preview.
const FILE_PREVIEW_LINES: usize = 200;

/// Background job that produces a preview for the file-explorer right pane.
///
/// - If `path` is a directory it reads the entries and returns them.
/// - If `path` is a (text) file it reads the first few KiB and returns
///   a line-trimmed preview string.
/// - Binary files are represented by a placeholder message.
#[derive(Debug)]
pub struct ExplorerPreviewJob {
    right_doc_id: DocumentId,
    path: PathBuf,
    show_hidden: bool,
}

impl ExplorerPreviewJob {
    pub fn new(right_doc_id: DocumentId, path: PathBuf, show_hidden: bool) -> Self {
        Self {
            right_doc_id,
            path,
            show_hidden,
        }
    }
}

impl Job for ExplorerPreviewJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        if self.path.is_dir() {
            // Directory preview: list entries
            let mut entries = Vec::new();
            if let Ok(read_dir) = fs::read_dir(&self.path) {
                for entry in read_dir.flatten() {
                    if signal.is_cancelled() {
                        return;
                    }
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !self.show_hidden && name.starts_with('.') {
                        continue;
                    }
                    let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
                    entries.push(DirEntry {
                        path: entry.path(),
                        is_dir,
                    });
                }
            }
            entries.sort_by(|a, b| {
                if a.is_dir != b.is_dir {
                    return b.is_dir.cmp(&a.is_dir);
                }
                a.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
                    .cmp(
                        &b.path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase(),
                    )
            });

            let result = Box::new(ExplorerPreviewResult {
                right_doc_id: self.right_doc_id,
                path: self.path,
                dir_entries: Some(entries),
                file_text: None,
            });
            let _ = sender.send(JobMessage::Custom(id, result));
            let _ = sender.send(JobMessage::Finished(id, true));
        } else {
            // File preview
            let text = match fs::File::open(&self.path) {
                Err(_) => "<cannot open file>".to_string(),
                Ok(mut file) => {
                    let mut buf = vec![0u8; FILE_PREVIEW_BYTES];
                    let n = file.read(&mut buf).unwrap_or(0);
                    let slice = &buf[..n];
                    match std::str::from_utf8(slice) {
                        Ok(s) => s
                            .lines()
                            .take(FILE_PREVIEW_LINES)
                            .collect::<Vec<_>>()
                            .join("\n"),
                        Err(_) => "<binary file>".to_string(),
                    }
                }
            };

            if signal.is_cancelled() {
                return;
            }

            let result = Box::new(ExplorerPreviewResult {
                right_doc_id: self.right_doc_id,
                path: self.path,
                dir_entries: None,
                file_text: Some(text),
            });
            let _ = sender.send(JobMessage::Custom(id, result));
            let _ = sender.send(JobMessage::Finished(id, true));
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_manager::{CancellationSignal, JobMessage};
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc};

    fn make_signal(cancelled: bool) -> CancellationSignal {
        CancellationSignal {
            cancelled: Arc::new(AtomicBool::new(cancelled)),
        }
    }

    #[test]
    fn test_explorer_preview_result_job_payload() {
        let r = ExplorerPreviewResult {
            right_doc_id: 5,
            path: PathBuf::from("/tmp"),
            dir_entries: Some(vec![]),
            file_text: None,
        };
        let boxed: Box<dyn JobPayload> = Box::new(r);
        assert!(boxed
            .as_any()
            .downcast_ref::<ExplorerPreviewResult>()
            .is_some());
    }

    #[test]
    fn test_explorer_preview_job_is_silent() {
        let job = ExplorerPreviewJob::new(1, PathBuf::from("/tmp"), false);
        assert!(job.is_silent());
    }

    #[test]
    fn test_explorer_preview_dir_returns_entries() {
        let tmp = std::env::temp_dir();
        let job = Box::new(ExplorerPreviewJob::new(42, tmp.clone(), false));
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

        let result = payload
            .into_any()
            .downcast::<ExplorerPreviewResult>()
            .expect("should be ExplorerPreviewResult");

        assert_eq!(result.right_doc_id, 42);
        assert_eq!(result.path, tmp);
        assert!(
            result.dir_entries.is_some(),
            "dir preview should have entries"
        );
        assert!(result.file_text.is_none());
    }

    #[test]
    fn test_explorer_preview_nonexistent_file_returns_placeholder() {
        let path = PathBuf::from("/nonexistent_path_xyz_rift_test/file.txt");
        let job = Box::new(ExplorerPreviewJob::new(1, path, false));
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

        let result = payload
            .into_any()
            .downcast::<ExplorerPreviewResult>()
            .unwrap();
        assert!(result.file_text.is_some());
        assert!(
            result
                .file_text
                .as_deref()
                .unwrap()
                .contains("cannot open file")
                || result.file_text.as_deref().unwrap().is_empty()
        );
    }

    #[test]
    fn test_explorer_preview_cancelled_before_run() {
        let job = Box::new(ExplorerPreviewJob::new(1, std::env::temp_dir(), false));
        let (tx, rx) = mpsc::channel();
        job.run(1, tx, make_signal(true));
        let msgs: Vec<JobMessage> = rx.try_iter().collect();
        assert!(!msgs.iter().any(|m| matches!(m, JobMessage::Custom(_, _))));
    }

    #[test]
    fn test_explorer_preview_dir_entries_sorted_dirs_first() {
        // Use the current working directory which should have some contents
        let dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
        let job = Box::new(ExplorerPreviewJob::new(1, dir, true));
        let (tx, rx) = mpsc::channel();
        job.run(1, tx, make_signal(false));

        let msgs: Vec<JobMessage> = rx.try_iter().collect();
        let payload = msgs.into_iter().find_map(|m| {
            if let JobMessage::Custom(_, p) = m {
                Some(p)
            } else {
                None
            }
        });
        if let Some(payload) = payload {
            let result = payload
                .into_any()
                .downcast::<ExplorerPreviewResult>()
                .unwrap();
            if let Some(entries) = result.dir_entries {
                // All directories should come before all files
                let mut saw_file = false;
                for entry in &entries {
                    if !entry.is_dir {
                        saw_file = true;
                    }
                    if saw_file && entry.is_dir {
                        panic!("directory found after file — sorting is wrong");
                    }
                }
            }
        }
    }

    #[test]
    fn test_explorer_preview_finished_message_is_sent() {
        let job = Box::new(ExplorerPreviewJob::new(1, std::env::temp_dir(), false));
        let (tx, rx) = mpsc::channel();
        job.run(1, tx, make_signal(false));
        let msgs: Vec<JobMessage> = rx.try_iter().collect();
        assert!(msgs
            .iter()
            .any(|m| matches!(m, JobMessage::Finished(1, true))));
    }
}
