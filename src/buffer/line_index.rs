//! Line indexing for TextBuffer
//! Tracks logical offsets of line starts

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    /// Logical offsets of the start of each line
    /// Always contains at least [0]
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Create a new LineIndex starting with a single empty line
    #[must_use]
    pub fn new() -> Self {
        Self {
            line_starts: vec![0],
        }
    }

    /// Total number of lines
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Get logical start of a line (0-indexed)
    #[must_use]
    pub fn get_start(&self, line_idx: usize) -> Option<usize> {
        self.line_starts.get(line_idx).copied()
    }

    /// Get logical end of a line (exclusive, position of the newline or end of buffer)
    /// Needs total_len to handle the last line correctly
    #[must_use]
    pub fn get_end(&self, line_idx: usize, total_len: usize) -> Option<usize> {
        if line_idx >= self.line_starts.len() {
            return None;
        }
        if line_idx + 1 < self.line_starts.len() {
            // End of this line is the start of the next line minus 1 (the newline)
            // But usually 'end' in ranges is exclusive, so next start is perfect
            Some(self.line_starts[line_idx + 1].saturating_sub(1))
        } else {
            // Last line
            Some(total_len)
        }
    }

    /// Get line number for a logical position (binary search)
    #[must_use]
    pub fn get_line_at(&self, pos: usize) -> usize {
        match self.line_starts.binary_search(&pos) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        }
    }

    /// Update index for insertion
    pub fn insert(&mut self, pos: usize, bytes: &[u8]) {
        let len = bytes.len();
        if len == 0 {
            return;
        }

        // Find which line we are inserting into
        let line_idx = self.get_line_at(pos);

        // Shift all subsequent line starts
        for start in self.line_starts.iter_mut().skip(line_idx + 1) {
            *start += len;
        }

        // Find newlines in the inserted text and add new line starts
        let mut new_starts = Vec::new();
        for (i, &byte) in bytes.iter().enumerate() {
            if byte == b'\n' {
                new_starts.push(pos + i + 1);
            }
        }

        if !new_starts.is_empty() {
            // Insert the new line starts in the correct place
            self.line_starts
                .splice(line_idx + 1..line_idx + 1, new_starts);
        }
    }

    /// Update index for deletion
    pub fn delete(&mut self, pos: usize, len: usize) {
        if len == 0 {
            return;
        }

        let delete_end = pos + len;

        // 1. Remove line starts that fall within the deleted range (exclusive of pos)
        // Any starts > pos and <= pos+len should be removed because those lines are merged.
        self.line_starts
            .retain(|&start| start <= pos || start > delete_end);

        // 2. Shift all starts after the deleted range
        for start in self.line_starts.iter_mut() {
            if *start > pos {
                *start -= len;
            }
        }
    }
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let idx = LineIndex::new();
        assert_eq!(idx.line_count(), 1);
        assert_eq!(idx.get_start(0), Some(0));
    }

    #[test]
    fn test_insert_no_newline() {
        let mut idx = LineIndex::new();
        idx.insert(0, b"abc");
        assert_eq!(idx.line_count(), 1);
        assert_eq!(idx.get_start(0), Some(0));
    }

    #[test]
    fn test_insert_newline() {
        let mut idx = LineIndex::new();
        idx.insert(0, b"a\nb");
        assert_eq!(idx.line_count(), 2);
        assert_eq!(idx.get_start(0), Some(0));
        assert_eq!(idx.get_start(1), Some(2));
    }

    #[test]
    fn test_insert_multiple_newlines() {
        let mut idx = LineIndex::new();
        idx.insert(0, b"a\nb\nc\n");
        assert_eq!(idx.line_count(), 4);
        assert_eq!(idx.get_start(0), Some(0));
        assert_eq!(idx.get_start(1), Some(2));
        assert_eq!(idx.get_start(2), Some(4));
        assert_eq!(idx.get_start(3), Some(6));
    }

    #[test]
    fn test_shift_after_insert() {
        let mut idx = LineIndex::new();
        idx.insert(0, b"a\nb"); // [0, 2]
        idx.insert(1, b"X"); // Insert X before newline: "aX\nb" -> [0, 3]
        assert_eq!(idx.line_count(), 2);
        assert_eq!(idx.get_start(1), Some(3));
    }

    #[test]
    fn test_delete_content() {
        let mut idx = LineIndex::new();
        idx.insert(0, b"abc\n"); // [0, 4]
        idx.delete(1, 1); // "ac\n" -> [0, 3]
        assert_eq!(idx.line_count(), 2);
        assert_eq!(idx.get_start(1), Some(3));
    }

    #[test]
    fn test_delete_newline() {
        let mut idx = LineIndex::new();
        idx.insert(0, b"a\nb"); // [0, 2]
        idx.delete(1, 1); // "ab" -> [0]
        assert_eq!(idx.line_count(), 1);
        assert_eq!(idx.get_start(0), Some(0));
    }

    #[test]
    fn test_get_line_at() {
        let mut idx = LineIndex::new();
        idx.insert(0, b"a\nbb\nccc"); // [0, 2, 5]
        assert_eq!(idx.get_line_at(0), 0);
        assert_eq!(idx.get_line_at(1), 0);
        assert_eq!(idx.get_line_at(2), 1);
        assert_eq!(idx.get_line_at(4), 1);
        assert_eq!(idx.get_line_at(5), 2);
        assert_eq!(idx.get_line_at(100), 2);
    }
}
