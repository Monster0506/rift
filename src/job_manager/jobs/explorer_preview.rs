use crate::document::{DirEntry, DocumentId};
use crate::job_manager::{CancellationSignal, Job, JobMessage};
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

crate::impl_job_payload!(ExplorerPreviewResult);

/// Maximum number of bytes read for a file preview.
const FILE_PREVIEW_BYTES: usize = 8 * 1024; // 8 KiB
/// Maximum number of lines shown in a file preview.
const FILE_PREVIEW_LINES: usize = 200;

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

/// Background job producing a file-explorer right-pane preview: directory
/// entries, a trimmed text snippet, or a placeholder message for binary files.
#[derive(Debug)]
pub struct ExplorerPreviewJob {
    right_doc_id: DocumentId,
    path: PathBuf,
    show_hidden: bool,
    token: Option<crate::job_manager::AsyncToken>,
}

impl ExplorerPreviewJob {
    pub fn new(right_doc_id: DocumentId, path: PathBuf, show_hidden: bool) -> Self {
        Self {
            right_doc_id,
            path,
            show_hidden,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }
}

impl Job for ExplorerPreviewJob {
    fn name(&self) -> &'static str {
        "explorer-preview"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<DocumentId> {
        Some(self.right_doc_id)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::ExplorerPreview)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        let fs = crate::fs_backend::backend();

        if fs.is_dir(&self.path) {
            let mut entries: Vec<DirEntry> = fs
                .list_children(&self.path)
                .unwrap_or_default()
                .into_iter()
                .filter(|c| self.show_hidden || !c.name.starts_with('.'))
                .map(|c| DirEntry {
                    path: self.path.join(&c.name),
                    is_dir: c.is_dir,
                    id: 0,
                })
                .collect();
            // sort_by_cached_key computes each entry's lowercase name
            // exactly once, keeping large listings fast.
            entries.sort_by_cached_key(|e| {
                (
                    !e.is_dir,
                    e.path
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
            if let Some(token) = self.token {
                crate::job_manager::send_job_result_with_token(&sender, id, token, result);
            } else {
                crate::job_manager::send_job_result(&sender, id, result);
            }
        } else {
            let text = match fs.read_file_prefix(&self.path, FILE_PREVIEW_BYTES) {
                Err(_) => "<cannot open file>".to_string(),
                Ok(bytes) => match decode_preview_text(&bytes) {
                    Some(s) => s
                        .lines()
                        .take(FILE_PREVIEW_LINES)
                        .collect::<Vec<_>>()
                        .join("\n"),
                    None => "<binary file>".to_string(),
                },
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
            if let Some(token) = self.token {
                crate::job_manager::send_job_result_with_token(&sender, id, token, result);
            } else {
                crate::job_manager::send_job_result(&sender, id, result);
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "explorer_preview_tests.rs"]
mod explorer_preview_tests;
