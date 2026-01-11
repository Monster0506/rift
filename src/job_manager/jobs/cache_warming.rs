use crate::buffer::byte_map::ByteLineMap;
use crate::buffer::rope::PieceTable;
use crate::character::Character;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::sync::mpsc::Sender;

/// Job to warm the search cache (byte offsets for lines)
#[derive(Debug)]
pub struct CacheWarmingJob {
    piece_table: PieceTable,
    revision: u64,
}

impl CacheWarmingJob {
    pub fn new(piece_table: PieceTable, revision: u64) -> Self {
        Self {
            piece_table,
            revision,
        }
    }
}

impl Job for CacheWarmingJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        let mut line_starts = Vec::new();
        let mut line_char_starts = Vec::new(); // New: Cache char offsets
        line_starts.push(0);
        line_char_starts.push(0);

        // Fast/optimized scanning using chunks
        let mut current_byte_offset = 0;
        let mut current_char_offset = 0;

        // We only need to check for newlines.
        // PieceTableChunkIterator yields &[Character].
        // Character can be Char(char) or Byte(u8).
        // Newline is Character::Newline (which is effectively '\n').

        for chunk in self.piece_table.chunks() {
            if signal.is_cancelled() {
                return;
            }

            for char in chunk {
                let len = char.len_utf8();
                current_byte_offset += len;
                current_char_offset += 1;

                if matches!(char, Character::Newline) {
                    line_starts.push(current_byte_offset);
                    line_char_starts.push(current_char_offset);
                }
            }
        }

        // Construct the result
        let result = ByteLineMap::new(line_starts, line_char_starts, self.revision);

        // Send back
        let _ = sender.send(JobMessage::Custom(id, Box::new(result)));
        let _ = sender.send(JobMessage::Finished(id, true));
    }

    fn is_silent(&self) -> bool {
        true
    }
}
