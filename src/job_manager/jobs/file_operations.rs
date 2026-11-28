use crate::buffer::line_index::LineIndex;
use crate::buffer::rope::PieceTable;
use crate::character::Character;
use crate::document::DocumentId;
use crate::document::LineEnding;
use crate::history::EditSeq;
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload, JobRef, JobTarget};
use sha2::{Digest, Sha256};
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
        }
    }
}

impl FileSaveJob {
    fn write(
        &self,
        signal: &CancellationSignal,
    ) -> std::io::Result<Option<crate::history::persist::Sha256Digest>> {
        if signal.is_cancelled() {
            return Ok(None);
        }
        let parent = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        let mut writer = std::io::BufWriter::new(temp.as_file_mut());
        let mut hash = Sha256::new();
        let line_ending = self.line_ending.as_bytes();
        if self.has_bom {
            writer.write_all(&crate::document::factories::UTF8_BOM)?;
        }
        let mut bytes = Vec::new();
        for chunk in self.piece_table.chunks() {
            if signal.is_cancelled() {
                return Ok(None);
            }
            bytes.reserve(chunk.len());
            for ch in chunk {
                if *ch == Character::Newline {
                    writer.write_all(&bytes)?;
                    writer.write_all(line_ending)?;
                    hash.update(&bytes);
                    hash.update(b"\n");
                    bytes.clear();
                } else {
                    ch.encode_utf8(&mut bytes);
                }
            }
            writer.write_all(&bytes)?;
            hash.update(&bytes);
            bytes.clear();
        }
        writer.flush()?;
        drop(writer);
        temp.as_file().sync_all()?;
        if signal.is_cancelled() {
            return Ok(None);
        }
        temp.persist(&self.path).map_err(|err| err.error)?;
        Ok(Some(hash.finalize().into()))
    }
}

impl Job for FileSaveJob {
    fn name(&self) -> &'static str {
        "file-save"
    }

    fn target_document_id(&self) -> Option<DocumentId> {
        Some(self.document_id)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::FileSave)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        crate::perf_span!("document_save", crate::perf::PerfFields::default());
        match self.write(&signal) {
            Ok(Some(content_hash)) => {
                let result = FileSaveResult {
                    document_id: self.document_id,
                    saved_seq: self.saved_seq,
                    path: self.path,
                    content_hash,
                };
                crate::job_manager::send_job_result(&sender, id, Box::new(result));
            }
            Ok(None) => {
                let _ = sender.send(JobMessage::Cancelled(id));
            }
            Err(e) => {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileLoadMode {
    Open,
    Reload,
    OpenAsText,
}

#[derive(Debug)]
pub(crate) struct FileLoadJob {
    document_id: DocumentId,
    path: PathBuf,
    mode: FileLoadMode,
}

impl FileLoadJob {
    #[must_use]
    pub(crate) const fn new(document_id: DocumentId, path: PathBuf, mode: FileLoadMode) -> Self {
        Self {
            document_id,
            path,
            mode,
        }
    }
}

impl Job for FileLoadJob {
    fn name(&self) -> &'static str {
        "file-load"
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

                if self.mode != FileLoadMode::OpenAsText {
                    if let Some(image) = crate::image_view::decode(&bytes) {
                        return Ok(Box::new(ImageLoadResult {
                            document_id: self.document_id,
                            image: std::sync::Arc::new(image),
                            is_reload: self.mode == FileLoadMode::Reload,
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
                    is_reload: self.mode == FileLoadMode::Reload,
                    content_hash,
                }))
            };
        match do_load() {
            Ok(result) => {
                if signal.is_cancelled() {
                    let _ = sender.send(JobMessage::Cancelled(id));
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
