//! Buffer-based movement using Character iterator
//!
//! These functions work with TextBuffer for insert mode navigation.

use super::classify::{classify_character, CharClass};
use crate::buffer::TextBuffer;

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
