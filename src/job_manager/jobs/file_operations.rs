//! --- File Operations ---
use crate::buffer::line_index::LineIndex;
use crate::buffer::rope::PieceTable;
use crate::character::Character;
use crate::document::DocumentId;
use crate::document::LineEnding;
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
/// Payload for a successful file save
#[derive(Debug)]
pub struct FileSaveResult {
    pub document_id: DocumentId,
    pub revision: u64,
    pub path: PathBuf,
}

impl JobPayload for FileSaveResult {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

/// Job to save a file asynchronously
#[derive(Debug)]
pub struct FileSaveJob {
    pub document_id: DocumentId,
    pub piece_table: PieceTable,
    pub path: PathBuf,
    pub line_ending: LineEnding,
    pub revision: u64,
}

impl FileSaveJob {
    pub fn new(
        document_id: DocumentId,
        piece_table: PieceTable,
        path: PathBuf,
        line_ending: LineEnding,
        revision: u64,
    ) -> Self {
        Self {
            document_id,
            piece_table,
            path,
            line_ending,
            revision,
        }
    }
}

impl Job for FileSaveJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let temp_path = parent.join(format!(
            ".{}.tmp",
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
                        revision: self.revision,
                        path: self.path.clone(),
                    };
                    let _ = sender.send(JobMessage::Custom(id, Box::new(result)));
                    let _ = sender.send(JobMessage::Finished(id, true));
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
}

impl JobPayload for FileLoadResult {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

/// Job to load a file asynchronously
#[derive(Debug)]
pub struct FileLoadJob {
    pub document_id: DocumentId,
    pub path: PathBuf,
}

impl FileLoadJob {
    pub fn new(document_id: DocumentId, path: PathBuf) -> Self {
        Self { document_id, path }
    }
}

impl Job for FileLoadJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        let do_load = || -> Result<FileLoadResult, std::io::Error> {
            let bytes = fs::read(&self.path)?;

            if signal.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Cancelled",
                ));
            }

            // Detect line endings and normalize
            let mut line_ending = LineEnding::LF;
            let mut normalized_chars = Vec::with_capacity(bytes.len());

            // Replicate logic from Document::from_file but constructing Vec<Character>
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    line_ending = LineEnding::CRLF;
                    normalized_chars.push(Character::Newline);
                    i += 2;
                } else {
                    if bytes[i] == b'\n' {
                        // Mixed line endings? assume LF if we see bare LF?
                        // Document::from_file only checks CRLF.
                        normalized_chars.push(Character::Newline);
                    } else {
                        normalized_chars.push(Character::from(bytes[i]));
                    }
                    i += 1;
                }
            }

            // We should use PieceTable::new with normalized_chars
            let piece_table = PieceTable::new(normalized_chars);
            let line_index = LineIndex { table: piece_table };

            Ok(FileLoadResult {
                document_id: self.document_id,
                line_index,
                line_ending,
                path: self.path.clone(),
            })
        };

        match do_load() {
            Ok(result) => {
                if !signal.is_cancelled() {
                    let _ = sender.send(JobMessage::Custom(id, Box::new(result)));
                    let _ = sender.send(JobMessage::Finished(id, true));
                } else {
                    let _ = sender.send(JobMessage::Cancelled(id));
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    let _ = sender.send(JobMessage::Cancelled(id));
                } else {
                    let _ = sender.send(JobMessage::Error(id, e.to_string()));
                }
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}
