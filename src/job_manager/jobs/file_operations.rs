use crate::buffer::line_index::LineIndex;
use crate::buffer::rope::PieceTable;
use crate::character::Character;
use crate::document::DocumentId;
use crate::document::LineEnding;
use crate::history::EditSeq;
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload, JobRef, JobTarget};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
#[derive(Debug)]
pub struct FileSaveResult {
    pub(crate) document_id: DocumentId,
    pub(crate) saved_seq: EditSeq,
    pub(crate) path: PathBuf,
    pub(crate) content_hash: crate::history::persist::Sha256Digest,
}

impl JobPayload for FileSaveResult {
    fn apply(self: Box<Self>, editor: &mut dyn JobTarget, job: JobRef) {
        editor.apply_file_save(job.id, job.token, *self);
    }
}

#[derive(Debug)]
pub(crate) struct FileSaveJob {
    document_id: DocumentId,
    piece_table: PieceTable,
    path: PathBuf,
    line_ending: LineEnding,
    has_bom: bool,
    saved_seq: EditSeq,
    token: Option<crate::job_manager::AsyncToken>,
}

impl FileSaveJob {
    #[must_use]
    pub(crate) const fn new(
        document_id: DocumentId,
        piece_table: PieceTable,
        path: PathBuf,
        line_ending: LineEnding,
        has_bom: bool,
        saved_seq: EditSeq,
    ) -> Self {
        Self {
            document_id,
            piece_table,
            path,
            line_ending,
            has_bom,
            saved_seq,
            token: None,
        }
    }
}

impl Job for FileSaveJob {
    fn name(&self) -> &'static str {
        "file-save"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<DocumentId> {
        Some(self.document_id)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::FileSave)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        crate::perf_span!("document_save", crate::perf::PerfFields::default());
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let temp_path = parent.join(format!(
            "{}~",
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
        ));

        let do_write = || -> std::io::Result<()> {
            let file = fs::File::create(&temp_path)?;
            let line_ending_bytes = self.line_ending.as_bytes();

            let mut writer = std::io::BufWriter::new(file);
            if self.has_bom {
                writer.write_all(&crate::document::factories::UTF8_BOM)?;
            }

            for chunk in self.piece_table.chunks() {
                if signal.is_cancelled() {
                    return Ok(());
                }

                let mut current_chunk_bytes = Vec::with_capacity(chunk.len());
                for ch in chunk {
                    if *ch == Character::Newline {
                        if !current_chunk_bytes.is_empty() {
                            writer.write_all(&current_chunk_bytes)?;
                            current_chunk_bytes.clear();
                        }
                        writer.write_all(line_ending_bytes)?;
                    } else {
                        ch.encode_utf8(&mut current_chunk_bytes);
                    }
                }
                if !current_chunk_bytes.is_empty() {
                    writer.write_all(&current_chunk_bytes)?;
                }
            }

            writer.flush()?;

            if signal.is_cancelled() {
                return Ok(());
            }

            writer.get_ref().sync_all()?;
            drop(writer); // Close file
            fs::rename(&temp_path, &self.path)?;

            Ok(())
        };

        match do_write() {
            Ok(()) => {
                if signal.is_cancelled() {
                    let _ = fs::remove_file(&temp_path);
                    let _ = sender.send(JobMessage::Cancelled(id));
                } else {
                    let result = FileSaveResult {
                        document_id: self.document_id,
                        saved_seq: self.saved_seq,
                        path: self.path.clone(),
                        content_hash: crate::history::persist::sha256(
                            &self.piece_table.to_logical_bytes(),
                        ),
                    };
                    if let Some(token) = self.token {
                        crate::job_manager::send_job_result_with_token(
                            &sender,
                            id,
                            token,
                            Box::new(result),
                        );
                    } else {
                        crate::job_manager::send_job_result(&sender, id, Box::new(result));
                    }
                }
            }
            Err(e) => {
                let _ = fs::remove_file(&temp_path);
                let _ = sender.send(JobMessage::Error(id, e.into()));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub struct FileLoadResult {
    pub(crate) document_id: DocumentId,
    pub(crate) line_index: LineIndex,
    pub(crate) line_ending: LineEnding,
    pub(crate) has_bom: bool,
    pub(crate) is_reload: bool,
    pub(crate) content_hash: crate::history::persist::Sha256Digest,
}

impl JobPayload for FileLoadResult {
    fn apply(self: Box<Self>, editor: &mut dyn JobTarget, job: JobRef) {
        editor.apply_file_load(job.id, job.token, *self);
    }
}

/// A file opened as an image buffer instead of text.
#[derive(Debug)]
pub struct ImageLoadResult {
    pub(crate) document_id: DocumentId,
    pub(crate) image: std::sync::Arc<crate::image_view::DecodedImage>,
    pub(crate) is_reload: bool,
}

impl JobPayload for ImageLoadResult {
    fn apply(self: Box<Self>, editor: &mut dyn JobTarget, job: JobRef) {
        editor.apply_image_load(job.id, job.token, *self);
    }
}

#[derive(Debug)]
pub(crate) struct FileLoadJob {
    document_id: DocumentId,
    path: PathBuf,
    is_reload: bool,
    /// Open supported images as image buffers rather than text.
    detect_image: bool,
    token: Option<crate::job_manager::AsyncToken>,
}

impl FileLoadJob {
    #[must_use]
    pub(crate) const fn new(document_id: DocumentId, path: PathBuf) -> Self {
        Self {
            document_id,
            path,
            is_reload: false,
            detect_image: true,
            token: None,
        }
    }

    /// Load of `path` as text even if it is an image (`:e` on an image buffer).
    #[must_use]
    pub(crate) const fn new_text(document_id: DocumentId, path: PathBuf) -> Self {
        Self {
            document_id,
            path,
            is_reload: false,
            detect_image: false,
            token: None,
        }
    }

    /// Re-reads an open buffer; image files reopen as images, so `:e` toggles picture and bytes.
    #[must_use]
    pub(crate) const fn new_reload(document_id: DocumentId, path: PathBuf) -> Self {
        Self {
            document_id,
            path,
            is_reload: true,
            detect_image: true,
            token: None,
        }
    }
}

impl Job for FileLoadJob {
    fn name(&self) -> &'static str {
        "file-load"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<DocumentId> {
        Some(self.document_id)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::FileLoad)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        let do_load =
            || -> Result<Box<dyn crate::job_manager::JobPayload>, crate::error::RiftError> {
                let bytes = crate::fs_backend::backend().read_file(&self.path)?;

                if signal.is_cancelled() {
                    return Err(crate::error::RiftError::new(
                        crate::error::ErrorType::Io,
                        "CANCELLED",
                        "Cancelled",
                    ));
                }

                if self.detect_image {
                    if let Some(image) = crate::image_view::decode(&bytes) {
                        return Ok(Box::new(ImageLoadResult {
                            document_id: self.document_id,
                            image: std::sync::Arc::new(image),
                            is_reload: self.is_reload,
                        }));
                    }
                }

                let decoded = crate::document::factories::decode_file_bytes(&bytes);
                let piece_table = PieceTable::new(decoded.chars);
                let content_hash = crate::history::persist::sha256(&piece_table.to_logical_bytes());
                let line_index = LineIndex::from_table_with_starts(piece_table, decoded.starts);

                Ok(Box::new(FileLoadResult {
                    document_id: self.document_id,
                    line_index,
                    line_ending: decoded.line_ending,
                    has_bom: decoded.has_bom,
                    is_reload: self.is_reload,
                    content_hash,
                }))
            };
        match do_load() {
            Ok(result) => {
                if signal.is_cancelled() {
                    let _ = sender.send(JobMessage::Cancelled(id));
                } else if let Some(token) = self.token {
                    crate::job_manager::send_job_result_with_token(&sender, id, token, result);
                } else {
                    crate::job_manager::send_job_result(&sender, id, result);
                }
            }
            Err(e) => {
                if e.code == "CANCELLED" {
                    let _ = sender.send(JobMessage::Cancelled(id));
                } else {
                    let _ = sender.send(JobMessage::Error(id, e));
                }
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "file_operations_tests.rs"]
mod file_operations_tests;
