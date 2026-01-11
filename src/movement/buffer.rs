//! Buffer-based movement using Character iterator
//!
//! These functions work with TextBuffer for insert mode navigation.

use super::classify::{classify_character, is_sentence_end, CharClass};
use crate::buffer::TextBuffer;
use crate::character::Character;

/// Move cursor forward by one word in the buffer
///
/// Returns `true` if the cursor moved, `false` if already at end
pub fn move_word_right(buffer: &mut TextBuffer) -> bool {
    let len = buffer.len();
    if buffer.cursor() >= len {
        return false;
    }
    let start_pos = buffer.cursor();

    if let Some(curr) = buffer.char_at(buffer.cursor()) {
        let start_class = classify_character(curr);

        // 1. Skip current word category
        while buffer.cursor() < len {
            match buffer.char_at(buffer.cursor()) {
                Some(c) if classify_character(c) == start_class => {
                    buffer.move_right();
                }
                _ => break,
            }
        }

        // 2. Skip whitespace if we weren't already on whitespace
        if start_class != CharClass::Whitespace {
            while buffer.cursor() < len {
                match buffer.char_at(buffer.cursor()) {
                    Some(c) if classify_character(c) == CharClass::Whitespace => {
                        buffer.move_right();
                    }
                    _ => break,
                }
            }
        }
    }

    buffer.cursor() != start_pos
}

/// Move cursor backward by one word in the buffer
///
/// Returns `true` if the cursor moved, `false` if already at beginning
pub fn move_word_left(buffer: &mut TextBuffer) -> bool {
    if buffer.cursor() == 0 {
        return false;
    }
    let start_pos = buffer.cursor();

    buffer.move_left();

    // 1. Skip whitespace backwards
    while buffer.cursor() > 0 {
        match buffer.char_at(buffer.cursor()) {
            Some(c) if classify_character(c) == CharClass::Whitespace => {
                buffer.move_left();
            }
            _ => break,
        }
    }

    // 2. Find start of current category
    if let Some(curr) = buffer.char_at(buffer.cursor()) {
        let target_class = classify_character(curr);
        if target_class == CharClass::Whitespace {
            // Still whitespace? Means start of file is whitespace
            return true;
        }

        while buffer.cursor() > 0 {
            let prev_pos = buffer.cursor() - 1;
            match buffer.char_at(prev_pos) {
                Some(pc) if classify_character(pc) == target_class => {
                    buffer.move_left();
                }
                _ => break,
            }
        }
    }

    buffer.cursor() != start_pos
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

    let mut pos = buffer.cursor();
    while pos < len {
        if let Some(c) = buffer.char_at(pos) {
            if is_terminator(c) {
                // Check if next char is whitespace or EOF (standard sentence definition)
                let next_pos = pos + 1;
                if next_pos >= len || buffer.char_at(next_pos).is_none_or(is_whitespace) {
                    // Found sentence boundary - skip to start of next sentence
                    pos = next_pos;
                    while pos < len {
                        if let Some(wc) = buffer.char_at(pos) {
                            if !is_whitespace(wc) {
                                break;
                            }
                        }
                        pos += 1;
                    }
                    let _ = buffer.set_cursor(pos);
                    return true;
                }
            } else if c == Character::Newline && pos > start_pos {
                let _ = buffer.set_cursor(pos + 1);
                return true;
            }
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

    let mut pos = buffer.cursor().saturating_sub(1);
    while pos > 0 {
        if let Some(c) = buffer.char_at(pos) {
            if is_terminator(c) {
                let next_pos = pos + 1;
                if next_pos < buffer.len() && buffer.char_at(next_pos).is_some_and(is_whitespace) {
                    // Found terminator + whitespace - skip to start of next sentence
                    let mut s = next_pos;
                    while s < buffer.len() {
                        if let Some(wc) = buffer.char_at(s) {
                            if !is_whitespace(wc) {
                                break;
                            }
                        }
                        s += 1;
                    }

                    if s < buffer.cursor() {
                        let _ = buffer.set_cursor(s);
                        return true;
                    }
                }
            }
        }
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
