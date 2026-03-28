//! --- File Operations ---
use crate::buffer::line_index::LineIndex;
use crate::buffer::rope::PieceTable;
use crate::character::Character;
use crate::document::DocumentId;
use crate::document::LineEnding;
use crate::history::EditSeq;
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload};
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
    pub saved_seq: EditSeq,
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
        }
    }
}

impl Job for FileSaveJob {
    fn name(&self) -> &'static str {
        "file-save"
    }

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
                        saved_seq: self.saved_seq,
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
    pub is_reload: bool,
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
    pub is_reload: bool,
}

impl FileLoadJob {
    pub fn new(document_id: DocumentId, path: PathBuf) -> Self {
        Self { document_id, path, is_reload: false }
    }

    pub fn new_reload(document_id: DocumentId, path: PathBuf) -> Self {
        Self { document_id, path, is_reload: true }
    }
}

impl Job for FileLoadJob {
    fn name(&self) -> &'static str {
        "file-load"
    }

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

            let mut remaining = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { &bytes[3..] } else { &bytes[..] };
            while !remaining.is_empty() {
                if remaining[0] == b'\r' {
                    if remaining.len() > 1 && remaining[1] == b'\n' {
                        line_ending = LineEnding::CRLF;
                        normalized_chars.push(Character::Newline);
                        remaining = &remaining[2..];
                    } else {
                        // Standalone \r — strip it
                        remaining = &remaining[1..];
                    }
                    continue;
                }

                match std::str::from_utf8(remaining) {
                    Ok(s) => {
                        let mut chars = s.chars().peekable();
                        while let Some(c) = chars.next() {
                            if c == '\r' {
                                if chars.peek() == Some(&'\n') {
                                    line_ending = LineEnding::CRLF;
                                    chars.next();
                                    normalized_chars.push(Character::Newline);
                                }
                                // else: standalone \r — strip it
                            } else {
                                normalized_chars.push(Character::from(c));
                            }
                        }
                        break;
                    }
                    Err(e) => {
                        let valid_up_to = e.valid_up_to();
                        // SAFETY: from_utf8 guarantees remaining[..valid_up_to] is valid UTF-8
                        let valid = unsafe {
                            std::str::from_utf8_unchecked(&remaining[..valid_up_to])
                        };
                        let mut chars = valid.chars().peekable();
                        while let Some(c) = chars.next() {
                            if c == '\r' {
                                if chars.peek() == Some(&'\n') {
                                    line_ending = LineEnding::CRLF;
                                    chars.next();
                                    normalized_chars.push(Character::Newline);
                                }
                                // else: standalone \r — strip it
                            } else {
                                normalized_chars.push(Character::from(c));
                            }
                        }
                        let error_len = e.error_len().unwrap_or(1);
                        for &b in &remaining[valid_up_to..valid_up_to + error_len] {
                            normalized_chars.push(Character::Byte(b));
                        }
                        remaining = &remaining[valid_up_to + error_len..];
                    }
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
                is_reload: self.is_reload,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Character;
    use crate::job_manager::{CancellationSignal, Job, JobMessage};
    use std::sync::{atomic::AtomicBool, mpsc, Arc};

    fn make_signal() -> CancellationSignal {
        CancellationSignal { cancelled: Arc::new(AtomicBool::new(false)) }
    }

    fn run_load_job(path: PathBuf) -> FileLoadResult {
        let (tx, rx) = mpsc::channel();
        let doc_id: crate::document::DocumentId = 42;
        let job = Box::new(FileLoadJob::new(doc_id, path));
        job.run(1, tx, make_signal());

        let mut result = None;
        for msg in rx {
            if let JobMessage::Custom(_, payload) = msg {
                result = payload
                    .into_any()
                    .downcast::<FileLoadResult>()
                    .ok()
                    .map(|b| *b);
            }
        }
        result.expect("FileLoadJob did not produce a FileLoadResult")
    }

    #[test]
    fn file_load_job_strips_utf8_bom() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bom.txt");
        std::fs::write(&path, b"\xEF\xBB\xBFhello").unwrap();

        let result = run_load_job(path);
        let chars: Vec<Character> = result.line_index.table.iter().collect();

        assert_eq!(chars.len(), 5);
        assert_eq!(chars[0], Character::Unicode('h'));
    }

    #[test]
    fn file_load_job_decodes_multibyte_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("em_dash.txt");
        std::fs::write(&path, "a—b").unwrap();

        let result = run_load_job(path);
        let chars: Vec<Character> = result.line_index.table.iter().collect();

        assert_eq!(chars.len(), 3);
        assert_eq!(chars[0], Character::Unicode('a'));
        assert_eq!(chars[1], Character::Unicode('—'));
        assert_eq!(chars[2], Character::Unicode('b'));
    }
}
