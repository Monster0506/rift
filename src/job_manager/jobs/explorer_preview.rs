use crate::document::{DirEntry, DocumentId};
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub struct ExplorerPreviewResult {
    pub right_doc_id: DocumentId,
    pub path: PathBuf,
    pub dir_entries: Option<Vec<DirEntry>>,
    pub file_text: Option<String>,
    pub undo_file: Option<crate::history::persist::ParsedUndoFile>,
}

crate::impl_job_payload!(ExplorerPreviewResult);

const FILE_PREVIEW_BYTES: usize = 8 * 1024; // 8 KiB
const CUSTOM_PREVIEW_BYTES: usize = 4 * 1024 * 1024; // 4 MiB
const FILE_PREVIEW_LINES: usize = 200;

fn decode_preview_text(slice: &[u8]) -> Option<&str> {
    match std::str::from_utf8(slice) {
        Ok(s) => Some(s),
        Err(err) if err.error_len().is_none() => {
            std::str::from_utf8(&slice[..err.valid_up_to()]).ok()
        }
        Err(_) => None,
    }
}

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
                undo_file: None,
            });
            if let Some(token) = self.token {
                crate::job_manager::send_job_result_with_token(&sender, id, token, result);
            } else {
                crate::job_manager::send_job_result(&sender, id, result);
            }
        } else {
            let prefix = fs.read_file_prefix(&self.path, FILE_PREVIEW_BYTES);
            let looks_like_undo_file = prefix
                .as_ref()
                .map(|bytes| crate::history::persist::has_magic(bytes))
                .unwrap_or(false);

            let (text, undo_file) = if looks_like_undo_file {
                let full = fs
                    .read_file_prefix(&self.path, CUSTOM_PREVIEW_BYTES)
                    .unwrap_or_default();
                match crate::document::Document::sniff_undo_file(&full) {
                    Some(Ok(parsed)) => (String::new(), Some(parsed)),
                    _ => ("<corrupt undo file>".to_string(), None),
                }
            } else {
                let text = match prefix {
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
                (text, None)
            };

            if signal.is_cancelled() {
                return;
            }

            let result = Box::new(ExplorerPreviewResult {
                right_doc_id: self.right_doc_id,
                path: self.path,
                dir_entries: None,
                file_text: undo_file.is_none().then_some(text),
                undo_file,
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
