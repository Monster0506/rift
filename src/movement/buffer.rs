//! Buffer-based movement using Character iterator
//!
//! These functions work with TextBuffer for insert mode navigation.

use super::classify::{classify_character, is_sentence_end, is_word_char, CharClass};
use crate::buffer::TextBuffer;
use crate::character::Character;

/// Chars fetched per rope lookup when scanning backward.
const REV_BLOCK: usize = 512;

/// Reverse char reader: yields the chars before `end` (end-1, end-2, ...),
/// fetching REV_BLOCK-sized blocks so each rope lookup amortizes.
struct RevChars<'a> {
    buffer: &'a TextBuffer,
    block: Vec<Character>,
    block_start: usize,
    idx: usize,
}

impl<'a> RevChars<'a> {
    fn new(buffer: &'a TextBuffer, end: usize) -> Self {
        Self {
            buffer,
            block: Vec::new(),
            block_start: end,
            idx: 0,
        }
    }
}

impl Iterator for RevChars<'_> {
    type Item = Character;

    fn next(&mut self) -> Option<Character> {
        if self.idx == 0 {
            if self.block_start == 0 {
                return None;
            }
            let start = self.block_start.saturating_sub(REV_BLOCK);
            self.block.clear();
            self.block
                .extend(self.buffer.iter_at(start).take(self.block_start - start));
            self.idx = self.block.len();
            self.block_start = start;
            if self.idx == 0 {
                return None;
            }
        }
        self.idx -= 1;
        Some(self.block[self.idx])
    }
}

/// Move cursor forward by one word in the buffer
///
/// Returns `true` if the cursor moved, `false` if already at end
pub fn move_word_right(buffer: &mut TextBuffer) -> bool {
    let len = buffer.len();
    if buffer.cursor() >= len {
        return false;
    }
    let start_pos = buffer.cursor();

    let mut iter = buffer.iter_at(start_pos);
    let mut pos = start_pos;
    let mut cur = iter.next();

    if let Some(first) = cur {
        let start_class = classify_character(first);

        // 1. Skip current word category
        while let Some(c) = cur {
            if classify_character(c) != start_class {
                break;
            }
            pos += 1;
            cur = iter.next();
        }

        // 2. Skip whitespace if we weren't already on whitespace
        if start_class != CharClass::Whitespace {
            while let Some(c) = cur {
                if classify_character(c) != CharClass::Whitespace {
                    break;
                }
                pos += 1;
                cur = iter.next();
            }
        }
    }

    let _ = buffer.set_cursor(pos);
    pos != start_pos
}

/// Move cursor to the end of the current or next word (vim's inclusive `e` motion).
///
/// Returns `true` if the cursor moved, `false` if already at end
pub fn move_word_end(buffer: &mut TextBuffer) -> bool {
    let len = buffer.len();
    if buffer.cursor() >= len {
        return false;
    }
    let start_pos = buffer.cursor();

    let mut iter = buffer.iter_at(start_pos);
    let mut pos = start_pos;
    let mut cur = iter.next();

    // If already on the last character of a word, step past it (and any
    // trailing whitespace) so we search for the *next* word's end.
    if let Some(curr) = cur {
        let curr_class = classify_character(curr);
        let next_class = iter.clone().next().map(classify_character);
        if curr_class != CharClass::Whitespace && next_class != Some(curr_class) {
            pos += 1;
            cur = iter.next();
        }
    }

    // Skip leading whitespace
    while let Some(c) = cur {
        if classify_character(c) != CharClass::Whitespace {
            break;
        }
        pos += 1;
        cur = iter.next();
    }

    if cur.is_none() {
        let _ = buffer.set_cursor(pos);
        return pos != start_pos;
    }

    if let Some(curr) = cur {
        let target_class = classify_character(curr);

        // Skip through the word's character class, then step back one
        // position so the cursor lands inclusively on the last character.
        while let Some(c) = cur {
            if classify_character(c) != target_class {
                break;
            }
            pos += 1;
            cur = iter.next();
        }
        pos = pos.saturating_sub(1);
    }

    let _ = buffer.set_cursor(pos);
    pos != start_pos
}

/// Move cursor backward by one word in the buffer
///
/// Returns `true` if the cursor moved, `false` if already at beginning
pub fn move_word_left(buffer: &mut TextBuffer) -> bool {
    if buffer.cursor() == 0 {
        return false;
    }
    let start_pos = buffer.cursor();

    let mut rev = RevChars::new(buffer, start_pos);
    let mut pos = start_pos - 1;
    // Invariant: cur is the char at pos; rev yields the char at pos - 1 next.
    let mut cur = rev.next();

    // 1. Skip whitespace backwards
    while pos > 0 {
        match cur {
            Some(c) if classify_character(c) == CharClass::Whitespace => {
                pos -= 1;
                cur = rev.next();
            }
            _ => break,
        }
    }

    // 2. Find start of current category
    if let Some(curr) = cur {
        let target_class = classify_character(curr);
        if target_class == CharClass::Whitespace {
            // Still whitespace? Means start of file is whitespace
            let _ = buffer.set_cursor(pos);
            return true;
        }

        while pos > 0 {
            match rev.next() {
                Some(pc) if classify_character(pc) == target_class => {
                    pos -= 1;
                }
                _ => break,
            }
        }
    }

    let _ = buffer.set_cursor(pos);
    pos != start_pos
}

/// Move cursor forward by one WORD (whitespace-delimited, no punctuation boundary)
///
/// Returns `true` if the cursor moved, `false` if already at end
pub fn move_big_word_right(buffer: &mut TextBuffer) -> bool {
    let len = buffer.len();
    if buffer.cursor() >= len {
        return false;
    }
    let start_pos = buffer.cursor();

    let mut iter = buffer.iter_at(start_pos);
    let mut pos = start_pos;
    let mut cur = iter.next();

    if let Some(curr) = cur {
        let on_whitespace = !is_word_char(curr.to_char_lossy());

        // 1. Skip the current run (non-whitespace or whitespace)
        while let Some(c) = cur {
            if is_word_char(c.to_char_lossy()) == on_whitespace {
                break;
            }
            pos += 1;
            cur = iter.next();
        }

        // 2. Skip trailing whitespace if we started on a non-whitespace run
        if !on_whitespace {
            while let Some(c) = cur {
                if is_word_char(c.to_char_lossy()) {
                    break;
                }
                pos += 1;
                cur = iter.next();
            }
        }
    }

    let _ = buffer.set_cursor(pos);
    pos != start_pos
}

/// Move cursor backward by one WORD (whitespace-delimited, no punctuation boundary)
///
/// Returns `true` if the cursor moved, `false` if already at beginning
pub fn move_big_word_left(buffer: &mut TextBuffer) -> bool {
    if buffer.cursor() == 0 {
        return false;
    }
    let start_pos = buffer.cursor();

    let mut rev = RevChars::new(buffer, start_pos);
    let mut pos = start_pos - 1;
    // Invariant: cur is the char at pos; rev yields the char at pos - 1 next.
    let mut cur = rev.next();

    // 1. Skip whitespace backwards
    while pos > 0 {
        match cur {
            Some(c) if !is_word_char(c.to_char_lossy()) => {
                pos -= 1;
                cur = rev.next();
            }
            _ => break,
        }
    }

    // 2. Find start of current non-whitespace run
    if let Some(curr) = cur {
        if !is_word_char(curr.to_char_lossy()) {
            // Still whitespace? Means start of file is whitespace
            let _ = buffer.set_cursor(pos);
            return true;
        }

        while pos > 0 {
            match rev.next() {
                Some(pc) if is_word_char(pc.to_char_lossy()) => {
                    pos -= 1;
                }
                _ => break,
            }
        }
    }

    let _ = buffer.set_cursor(pos);
    pos != start_pos
}

/// Move cursor forward to the next sentence
///
/// Returns `true` if the cursor moved, `false` if already at end
pub fn move_sentence_forward(buffer: &mut TextBuffer) -> bool {
    let len = buffer.len();
    if buffer.cursor() >= len {
        return false;
    }
    let start_pos = buffer.cursor();

    let is_terminator = |c: Character| matches!(c, Character::Unicode(ch) if is_sentence_end(ch));
    let is_whitespace = |c: Character| {
        matches!(c, Character::Unicode(ch) if ch.is_whitespace())
            || matches!(c, Character::Tab | Character::Newline)
    };

    let mut iter = buffer.iter_at(start_pos).peekable();
    let mut pos = start_pos;
    while let Some(c) = iter.next() {
        if is_terminator(c) {
            // Check if next char is whitespace or EOF (standard sentence definition)
            if iter.peek().copied().is_none_or(is_whitespace) {
                // Found sentence boundary - skip to start of next sentence
                let mut target = pos + 1;
                for wc in iter.by_ref() {
                    if !is_whitespace(wc) {
                        break;
                    }
                    target += 1;
                }
                let _ = buffer.set_cursor(target.min(len));
                return true;
            }
        } else if c == Character::Newline && pos > start_pos {
            let _ = buffer.set_cursor(pos + 1);
            return true;
        }
        pos += 1;
    }

    buffer.move_to_end();
    buffer.cursor() != start_pos
}

/// Move cursor backward to the previous sentence
///
/// Returns `true` if the cursor moved, `false` if already at beginning
pub fn move_sentence_backward(buffer: &mut TextBuffer) -> bool {
    if buffer.cursor() == 0 {
        return false;
    }
    let start_pos = buffer.cursor();

    let is_terminator = |c: Character| matches!(c, Character::Unicode(ch) if is_sentence_end(ch));
    let is_whitespace = |c: Character| {
        matches!(c, Character::Unicode(ch) if ch.is_whitespace())
            || matches!(c, Character::Tab | Character::Newline)
    };

    let mut rev = RevChars::new(buffer, start_pos);
    let mut pos = start_pos - 1;
    // Invariant: next_c is the char at pos + 1 (None at end of buffer).
    let mut next_c = buffer.char_at(start_pos);
    while pos > 0 {
        let Some(c) = rev.next() else { break };
        if is_terminator(c) {
            if next_c.is_some_and(is_whitespace) {
                // Found terminator + whitespace - skip to start of next sentence
                let mut s = pos + 1;
                for wc in buffer.iter_at(pos + 1) {
                    if !is_whitespace(wc) {
                        break;
                    }
                    s += 1;
                }

                if s < start_pos {
                    let _ = buffer.set_cursor(s);
                    return true;
                }
            }
        } else if c == Character::Newline && pos + 1 < start_pos {
            let _ = buffer.set_cursor(pos + 1);
            return true;
        }
        next_c = Some(c);
        pos -= 1;
    }

    buffer.move_to_start();
    buffer.cursor() != start_pos
}

/// Move cursor forward to the next paragraph
///
/// Returns `true` if the cursor moved, `false` if already at end
pub fn move_paragraph_forward(buffer: &mut TextBuffer) -> bool {
    let current_line = buffer.get_line();
    let total_lines = buffer.get_total_lines();
    let start_pos = buffer.cursor();

    let mut target_line = current_line + 1;
    while target_line < total_lines {
        let start = buffer.line_index.get_start(target_line).unwrap_or(0);
        let end = buffer
            .line_index
            .get_end(target_line, buffer.len())
            .unwrap_or(buffer.len());

        // Check if this is an empty line (paragraph boundary)
        if end <= start {
            let _ = buffer.set_cursor(start);
            return true;
        }
        target_line += 1;
    }

    // If no empty line found, move to end
    buffer.move_to_end();
    buffer.cursor() != start_pos
}

/// Move cursor backward to the previous paragraph
///
/// Returns `true` if the cursor moved, `false` if already at beginning
pub fn move_paragraph_backward(buffer: &mut TextBuffer) -> bool {
    let current_line = buffer.get_line();
    let start_pos = buffer.cursor();

    if current_line == 0 {
        let _ = buffer.set_cursor(0);
        return buffer.cursor() != start_pos;
    }

    let mut target_line = current_line - 1;
    while target_line > 0 {
        let start = buffer.line_index.get_start(target_line).unwrap_or(0);
        let end = buffer
            .line_index
            .get_end(target_line, buffer.len())
            .unwrap_or(buffer.len());

        // Check if this is an empty line (paragraph boundary)
        if end <= start {
            let _ = buffer.set_cursor(start);
            return true;
        }
        target_line -= 1;
    }

    // If no empty line found, move to start
    buffer.move_to_start();
    buffer.cursor() != start_pos
}
