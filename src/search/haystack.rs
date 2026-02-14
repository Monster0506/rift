use crate::buffer::api::BufferView;
use crate::character::Character;
use monster_regex::{Haystack, HaystackCursor};

/// Context that holds the necessary indices for the haystack.
/// This struct owns the "Shadow Byte Index".
pub struct BufferHaystackContext<'a, B: BufferView + ?Sized> {
    buffer: &'a B,
    line_byte_starts: Vec<usize>,
    line_char_starts: Vec<usize>,
}

impl<'a, B: BufferView + ?Sized> BufferHaystackContext<'a, B> {
    pub fn new(buffer: &'a B) -> Self {
        // Try to use cached byte map
        if let Some(cell) = buffer.byte_line_map() {
            let mut cache = cell.borrow_mut();
            let current_rev = buffer.revision();

            // Check cache validity (revision match)
            if let Some(map) = cache.as_ref() {
                if map.revision == current_rev {
                    // Cache hit!
                    return Self {
                        buffer,
                        line_byte_starts: map.line_starts.clone(),
                        line_char_starts: map.line_char_starts.clone(),
                    };
                }
            }

            // Cache miss or stale: rebuild
            let line_count = buffer.line_count();
            let mut line_byte_starts = Vec::with_capacity(line_count + 1);
            let mut line_char_starts = Vec::with_capacity(line_count + 1);
            let mut current_offset = 0;
            let mut current_char_offset = 0; // Track char offset
            line_byte_starts.push(0);
            line_char_starts.push(0);

            for i in 0..line_count {
                let start = buffer.line_start(i);
                let end = if i + 1 < line_count {
                    buffer.line_start(i + 1)
                } else {
                    buffer.len()
                };
                let mut line_len = 0;
                let mut line_char_len = 0;
                for c in buffer.chars(start..end) {
                    line_len += c.len_utf8();
                    line_char_len += 1;
                }
                current_offset += line_len;
                current_char_offset += line_char_len;
                line_byte_starts.push(current_offset);
                line_char_starts.push(current_char_offset);
            }

            // Update cache
            *cache = Some(crate::buffer::byte_map::ByteLineMap::new(
                line_byte_starts.clone(),
                line_char_starts.clone(),
                current_rev,
            ));

            return Self {
                buffer,
                line_byte_starts,
                line_char_starts,
            };
        }

        // No cache available fallback (unlikely in Editor but possible in tests)
        let line_count = buffer.line_count();
        let mut line_byte_starts = Vec::with_capacity(line_count + 1);
        let mut line_char_starts = Vec::with_capacity(line_count + 1);

        line_byte_starts.push(0);
        line_char_starts.push(0);
        // Note: In fallback without cache, we might not have efficient way to get char starts without scanning.
        // But this path is for "BufferView without byte_line_map support" (e.g. MockBuffer).
        // MockBuffer is small, so we can iterate.
        // Or we can rely on `buffer.line_start` if available.

        for i in 0..line_count {
            let next_line_start_char = if i + 1 < line_count {
                buffer.line_start(i + 1)
            } else {
                buffer.len()
            };

            let next_line_start_byte = buffer.char_to_byte(next_line_start_char);

            line_byte_starts.push(next_line_start_byte);
            line_char_starts.push(next_line_start_char);
        }

        Self {
            buffer,
            line_byte_starts,
            line_char_starts,
        }
    }

    pub fn make_haystack(&'a self) -> BufferHaystack<'a, B> {
        BufferHaystack {
            buffer: self.buffer,
            line_byte_starts: &self.line_byte_starts,
            line_char_starts: &self.line_char_starts,
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
    line_char_starts: &'a [usize],
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

        // Reached end of line matching region
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

#[derive(Clone)]
pub struct BufferCursor<I: Iterator<Item = Character> + Clone> {
    iter: I,
    peeked: Option<char>, // Cache for peek
}

impl<I: Iterator<Item = Character> + Clone> BufferCursor<I> {
    fn new(iter: I) -> Self {
        Self { iter, peeked: None }
    }
}

impl<I: Iterator<Item = Character> + Clone> Iterator for BufferCursor<I> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c) = self.peeked.take() {
            return Some(c);
        }
        self.iter.next().map(|c| c.to_char_lossy())
    }
}

impl<I: Iterator<Item = Character> + Clone> HaystackCursor for BufferCursor<I> {
    fn peek(&self) -> Option<char> {
        let mut iter = self.iter.clone();
        iter.next().map(|c| c.to_char_lossy())
    }
}

impl<'a, B: BufferView + ?Sized> Haystack for BufferHaystack<'a, B> {
    type Cursor = BufferCursor<B::CharIter<'a>>;

    fn len(&self) -> usize {
        *self.line_byte_starts.last().unwrap_or(&0)
    }

    fn cursor_at(&self, pos: usize) -> Self::Cursor {
        let char_pos = self.byte_offset_to_char_abs(pos);
        BufferCursor::new(self.buffer.iter_at(char_pos))
    }

    fn char_at(&self, pos: usize) -> Option<(char, usize)> {
        if pos >= self.len() {
            return None;
        }

        let (line_idx, offset_in_line) = self.find_line_for_byte(pos)?;

        // Critical Optimization: Use cached char start for the line
        // buffer.line_start(i) is slow for huge lines/files (O(N) scan).
        // line_char_starts[line_idx] is O(1) lookup.
        let line_start_char_idx = self.line_char_starts[line_idx];

        // offset_in_line is byte offset within the line.
        // We need to find the character at that byte offset.

        let mut current_byte = 0;
        // Start iterating from the *line start* (using fast iter_at via char index)
        // buffer.iter_at is O(log N)
        // This avoids scanning from the beginning of a huge piece.
        let iter = self.buffer.iter_at(line_start_char_idx);

        for c in iter {
            if current_byte == offset_in_line {
                return Some((c.to_char_lossy(), c.len_utf8()));
            }
            current_byte += c.len_utf8();
            if current_byte > offset_in_line {
                return None; // Mid-character
            }
            // Check if we passed newline (end of line scope for this haystack logic)
            // Actually, haystack treats the whole file as contiguous bytes,
            // but `find_line_for_byte` segments it.
            // We just need to ensure we don't go forever.
            // The loop naturally terminates if we overshoot offset_in_line.
            // But for safety/correctness if multiple lines involved?
            // find_line_for_byte guarantees pos is within line boundary bytes.
        }

        None
    }

    fn char_before(&self, pos: usize) -> Option<char> {
        if pos == 0 {
            return None;
        }
        let (line_idx, offset_in_line_prev) = self.find_line_for_byte(pos - 1)?;

        let line_start_char_idx = self.line_char_starts[line_idx];
        let iter = self.buffer.iter_at(line_start_char_idx);

        let mut current_byte = 0;
        for c in iter {
            let len = c.len_utf8();
            if current_byte + len == offset_in_line_prev + 1 {
                return Some(c.to_char_lossy());
            }
            current_byte += len;
            if current_byte > offset_in_line_prev {
                break;
            }
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

    fn find_byte(&self, byte: u8, pos: usize) -> Option<usize> {
        if pos >= *self.line_byte_starts.last()? {
            return None;
        }

        // Find line for start byte
        let (mut line_idx, mut offset_in_line) = self.find_line_for_byte(pos)?;

        // Iterate line by line
        while line_idx < self.buffer.line_count() {
            let line_start_char_idx = self.line_char_starts[line_idx];
            // Iterate from start of this line
            let iter = self.buffer.iter_at(line_start_char_idx);

            let mut current_byte = 0;
            let mut moved_to_next_line = false;

            for c in iter {
                let len = c.len_utf8();

                if current_byte >= offset_in_line {
                    match c {
                        Character::Unicode(ch) => {
                            let mut buf = [0u8; 4];
                            let s = ch.encode_utf8(&mut buf);
                            if let Some(idx) = s.as_bytes().iter().position(|&b| b == byte) {
                                let offset_from_line_start = self.line_byte_starts[line_idx];
                                return Some(offset_from_line_start + current_byte + idx);
                            }
                        }
                        Character::Byte(b) => {
                            if b == byte {
                                let offset_from_line_start = self.line_byte_starts[line_idx];
                                return Some(offset_from_line_start + current_byte);
                            }
                        }
                        Character::Tab => {
                            if byte == b'\t' {
                                let offset_from_line_start = self.line_byte_starts[line_idx];
                                return Some(offset_from_line_start + current_byte);
                            }
                        }
                        Character::Newline => {
                            if byte == b'\n' {
                                let offset_from_line_start = self.line_byte_starts[line_idx];
                                return Some(offset_from_line_start + current_byte);
                            }
                            // Newline marks end of this line; advance to next line.
                            line_idx += 1;
                            offset_in_line = 0;
                            moved_to_next_line = true;
                            break;
                        }
                        Character::Control(b) => {
                            if b == byte {
                                let offset_from_line_start = self.line_byte_starts[line_idx];
                                return Some(offset_from_line_start + current_byte);
                            }
                        }
                    }
                }
                current_byte += len;
            }

            if !moved_to_next_line {
                break;
            }

            if line_idx >= self.line_char_starts.len() {
                break;
            }
        }

        None
    }
}
