use crate::buffer::api::BufferView;
use monster_regex::Haystack;

/// Context that holds the necessary indices for the haystack.
/// This struct owns the "Shadow Byte Index".
pub struct BufferHaystackContext<'a, B: BufferView + ?Sized> {
    buffer: &'a B,
    line_byte_starts: Vec<usize>,
}

impl<'a, B: BufferView + ?Sized> BufferHaystackContext<'a, B> {
    pub fn new(buffer: &'a B) -> Self {
        let line_count = buffer.line_count();
        let mut line_byte_starts = Vec::with_capacity(line_count + 1);
        let mut current_offset = 0;

        line_byte_starts.push(0);
        for i in 0..line_count {
            let start = buffer.line_start(i);
            let end = if i + 1 < line_count {
                buffer.line_start(i + 1)
            } else {
                buffer.len()
            };

            let mut line_len = 0;
            // Iterate chars in the line. Note: Rift lines usually don't include the newline char
            // if it's implicitly managed, but `chars` range covers what's in buffer.
            // If the buffer HAS newlines in the text, `chars` will yield them?
            // `BufferView` usually represents the whole text.
            // `line_start` to `line_start` logic usually implies contiguous ranges.
            // If `line_start(i+1) - line_start(i)` includes the newline, `chars` includes it.
            // But `BufferHaystack` logic previously added +1 for newline.
            // If `BufferView` already provides newlines, we shouldn't add another one!
            // Let's verify `BufferView` contract.
            // `TextBuffer` uses PieceTable.
            // `get_line_bytes` excluded trailing newline.
            // `chars(range)` gets everything in range.
            // If the gap buffer/piece table stores newlines, they are in the range.
            // `TextBuffer` usually stores newlines.

            // Wait! `TextBuffer::get_line_bytes` documentation: "excluding trailing newline".
            // This suggests newlines ARE stored but `get_line_bytes` strips them or `line_index` tracks them.
            // If I iterate `chars(line_start(i)..line_start(i+1))`, do I get the newline?
            // `line_start` are code-point offsets.
            // If line 0 is "abc\n", len is 4. line_start(0)=0, line_start(1)=4.
            // chars(0..4) -> 'a', 'b', 'c', '\n'.
            // So YES, I get the newline.
            // So my previous `+1` was WRONG if I use `chars(range)` over the whole buffer ranges.
            // BUT `BufferHaystack` logic relies on `line_byte_starts`.
            // If I just map 0..len code points to 0..bytes, I don't need line structure?
            // I need line structure to efficiently find "where is Char X".
            // `BufferView` provides `line_start` (code point).
            // `monster-regex` needs byte offsets.
            // So `BufferHaystack` needs to map `ByteOffset -> CharOffset`.
            // If I assume `chars(range)` covers the whole buffer contiguously,
            // I can just map chunks?
            // But `BufferView` doesn't expose chunks (except via `iter` on TextBuffer, not trait).
            // So iterating lines is a good way to break it down.

            for c in buffer.chars(start..end) {
                line_len += c.len_utf8();
            }

            // If the buffer actually contains newlines, we accumulate them here.
            current_offset += line_len;
            line_byte_starts.push(current_offset);
        }

        Self {
            buffer,
            line_byte_starts,
        }
    }

    pub fn make_haystack(&'a self) -> BufferHaystack<'a, B> {
        BufferHaystack {
            buffer: self.buffer,
            line_byte_starts: &self.line_byte_starts,
        }
    }
}

// ... Copy/Clone impls ...
impl<'a, B: BufferView + ?Sized> Clone for BufferHaystack<'a, B> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'a, B: BufferView + ?Sized> Copy for BufferHaystack<'a, B> {}

pub struct BufferHaystack<'a, B: BufferView + ?Sized> {
    buffer: &'a B,
    line_byte_starts: &'a [usize],
}

impl<'a, B: BufferView + ?Sized> BufferHaystack<'a, B> {
    pub fn byte_offset_to_char_abs(&self, byte_pos: usize) -> usize {
        let (line_idx, offset_in_line) = match self.find_line_for_byte(byte_pos) {
            Some(v) => v,
            None => return self.buffer.len(),
        };

        let line_start_char = self.buffer.line_start(line_idx);
        let start = line_start_char;
        let end = if line_idx + 1 < self.buffer.line_count() {
            self.buffer.line_start(line_idx + 1)
        } else {
            self.buffer.len()
        };

        let mut current_byte = 0;
        let mut char_count = 0;

        for c in self.buffer.chars(start..end) {
            if current_byte == offset_in_line {
                return line_start_char + char_count;
            }
            let len = c.len_utf8();
            if current_byte + len > offset_in_line {
                // Pointing into middle of char? Should not happen for valid regex match boundaries.
                return line_start_char + char_count;
            }
            current_byte += len;
            char_count += 1;
        }

        // If we reached here, maybe it's the end of line index?
        line_start_char + char_count
    }

    fn find_line_for_byte(&self, byte_pos: usize) -> Option<(usize, usize)> {
        if byte_pos >= *self.line_byte_starts.last()? {
            return None;
        }

        match self.line_byte_starts.binary_search(&byte_pos) {
            Ok(idx) => Some((idx, 0)),
            Err(idx) => {
                let line_idx = idx - 1;
                let start_of_line = self.line_byte_starts[line_idx];
                Some((line_idx, byte_pos - start_of_line))
            }
        }
    }
}

impl<'a, B: BufferView + ?Sized> Haystack for BufferHaystack<'a, B> {
    fn len(&self) -> usize {
        *self.line_byte_starts.last().unwrap_or(&0)
    }

    fn char_at(&self, pos: usize) -> Option<(char, usize)> {
        if pos >= self.len() {
            return None;
        }

        let (line_idx, offset_in_line) = self.find_line_for_byte(pos)?;

        let start = self.buffer.line_start(line_idx);
        let end = if line_idx + 1 < self.buffer.line_count() {
            self.buffer.line_start(line_idx + 1)
        } else {
            self.buffer.len()
        };

        let mut current_byte = 0;
        for c in self.buffer.chars(start..end) {
            if current_byte == offset_in_line {
                return Some((c.to_char_lossy(), c.len_utf8()));
            }
            current_byte += c.len_utf8();
            if current_byte > offset_in_line {
                // Moved past it?
                return None;
            }
        }

        None
    }

    fn char_before(&self, pos: usize) -> Option<char> {
        if pos == 0 {
            return None;
        }

        // To find char ending at pos, we want char at (pos - prev_char_len).
        // Since we don't know prev_char_len, we must scan from start of line or use `find_line_for_byte`
        // to find where the char *containing* pos-1 starts.

        let (line_idx, offset_in_line_prev) = self.find_line_for_byte(pos - 1)?;

        let start = self.buffer.line_start(line_idx);
        let end = if line_idx + 1 < self.buffer.line_count() {
            self.buffer.line_start(line_idx + 1)
        } else {
            self.buffer.len()
        };

        let mut current_byte = 0;
        for c in self.buffer.chars(start..end) {
            let len = c.len_utf8();
            // If this char includes the byte at offset_in_line_prev (which is pos-1)
            // It means this char effectively "ends" at `current_byte + len`.
            // Does it end at `pos`?
            // If `current_byte + len` == `offset_in_line_prev + 1` relative to line start?
            // Actually offset_in_line_prev is relative to line start.
            // We want char that ends at `pos`.
            // In terms of line offsets: char ends at `offset_in_line_prev + 1`.

            // `offset_in_line_prev` points to the last byte of the previous char (if pos is char boundary).
            // So `current_byte + len` should be `offset_in_line_prev + 1`.

            // Wait. `pos` is after the char we want.
            // `pos-1` is the last byte of the char we want.
            // `offset_in_line_prev` is `pos-1` relative to line.

            if current_byte + len == offset_in_line_prev + 1 {
                return Some(c.to_char_lossy());
            }
            current_byte += len;
        }

        None
    }

    fn starts_with(&self, pos: usize, literal: &str) -> bool {
        let mut current_pos = pos;
        for c in literal.chars() {
            match self.char_at(current_pos) {
                Some((hc, len)) => {
                    if hc != c {
                        return false;
                    }
                    current_pos += len;
                }
                None => return false,
            }
        }
        true
    }

    fn matches_range(&self, pos: usize, other_start: usize, other_end: usize) -> bool {
        let mut p1 = pos;
        let mut p2 = other_start;

        while p2 < other_end {
            let c1 = self.char_at(p1);
            let c2 = self.char_at(p2);

            match (c1, c2) {
                (Some((ch1, l1)), Some((ch2, l2))) => {
                    if ch1 != ch2 {
                        return false;
                    }
                    p1 += l1;
                    p2 += l2;
                }
                _ => return false,
            }
        }
        true
    }
}
