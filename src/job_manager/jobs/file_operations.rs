//! --- File Operations ---
use crate::buffer::line_index::LineIndex;
use crate::buffer::rope::PieceTable;
use crate::character::Character;
use crate::document::DocumentId;
use crate::document::LineEnding;
use crate::history::EditSeq;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
/// Payload for a successful file save
#[derive(Debug)]
pub struct FileSaveResult {
    pub document_id: DocumentId,
    pub saved_seq: EditSeq,
    pub path: PathBuf,
}

crate::impl_job_payload!(FileSaveResult);

/// Job to save a file asynchronously
#[derive(Debug)]
pub struct FileSaveJob {
    pub document_id: DocumentId,
    pub piece_table: PieceTable,
    pub path: PathBuf,
    pub line_ending: LineEnding,
    pub saved_seq: EditSeq,
    pub token: Option<crate::job_manager::AsyncToken>,
}

impl FileSaveJob {
    pub fn new(
        document_id: DocumentId,
        piece_table: PieceTable,
        path: PathBuf,
        line_ending: LineEnding,
        saved_seq: EditSeq,
    ) -> Self {
        Self {
            document_id,
            piece_table,
            path,
            line_ending,
            saved_seq,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
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

        // Helper to handle IO errors and send Error message
        let do_write = || -> std::io::Result<()> {
            let file = fs::File::create(&temp_path)?;
            let line_ending_bytes = self.line_ending.as_bytes();

            // Buffering for performance
            let mut writer = std::io::BufWriter::new(file);

            for chunk in self.piece_table.chunks() {
                if signal.is_cancelled() {
                    return Ok(());
                }

                let mut current_chunk_bytes = Vec::with_capacity(chunk.len());
                for ch in chunk {
                    if *ch == Character::Newline {
                        // Flush current pending bytes
                        if !current_chunk_bytes.is_empty() {
                            writer.write_all(&current_chunk_bytes)?;
                            current_chunk_bytes.clear();
                        }
                        writer.write_all(line_ending_bytes)?;
                    } else {
                        // Encode char to bytes
                        ch.encode_utf8(&mut current_chunk_bytes);
                    }
                }
                if !current_chunk_bytes.is_empty() {
                    writer.write_all(&current_chunk_bytes)?;
                }
            }

            writer.flush()?;

            // Check cancellation before rename
            if signal.is_cancelled() {
                return Ok(());
            }

            // Sync and Rename
            writer.get_ref().sync_all()?;
            drop(writer); // Close file
            fs::rename(&temp_path, &self.path)?;

            Ok(())
        };

        match do_write() {
            Ok(()) => {
                if !signal.is_cancelled() {
                    let result = FileSaveResult {
                        document_id: self.document_id,
                        saved_seq: self.saved_seq,
                        path: self.path.clone(),
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
                } else {
                    // Clean up temp file
                    let _ = fs::remove_file(&temp_path);
                    let _ = sender.send(JobMessage::Cancelled(id));
                }
            }
            Err(e) => {
                // Try clean up temp file
                let _ = fs::remove_file(&temp_path);
                let _ = sender.send(JobMessage::Error(id, e.to_string()));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Payload for a successful file load
#[derive(Debug)]
pub struct FileLoadResult {
    pub document_id: DocumentId,
    pub line_index: LineIndex,
    pub line_ending: LineEnding,
    pub path: PathBuf,
    pub is_reload: bool,
}

crate::impl_job_payload!(FileLoadResult);

/// Job to load a file asynchronously
#[derive(Debug)]
pub struct FileLoadJob {
    pub document_id: DocumentId,
    pub path: PathBuf,
    pub is_reload: bool,
    pub token: Option<crate::job_manager::AsyncToken>,
}

impl FileLoadJob {
    pub fn new(document_id: DocumentId, path: PathBuf) -> Self {
        Self {
            document_id,
            path,
            is_reload: false,
            token: None,
        }
    }

    pub fn new_reload(document_id: DocumentId, path: PathBuf) -> Self {
        Self {
            document_id,
            path,
            is_reload: true,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
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
        let do_load = || -> Result<FileLoadResult, crate::error::RiftError> {
            let bytes = crate::fs_backend::backend().read_file(&self.path)?;

            if signal.is_cancelled() {
                return Err(crate::error::RiftError::new(
                    crate::error::ErrorType::Io,
                    "CANCELLED",
                    "Cancelled",
                ));
            }

            let (normalized_chars, line_ending, starts) =
                crate::document::decode_file_bytes(&bytes);
            let piece_table = PieceTable::new(normalized_chars);
            let line_index = LineIndex::from_table_with_starts(piece_table, starts);

            Ok(FileLoadResult {
                document_id: self.document_id,
                line_index,
                line_ending,
                path: self.path.clone(),
                is_reload: self.is_reload,
            })
        };

        match do_load() {
            Ok(result) => {
                if !signal.is_cancelled() {
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
                } else {
                    let _ = sender.send(JobMessage::Cancelled(id));
                }
            }
            Err(e) => {
                if e.code == "CANCELLED" {
                    let _ = sender.send(JobMessage::Cancelled(id));
                } else {
                    let _ = sender.send(JobMessage::Error(id, e.message));
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
