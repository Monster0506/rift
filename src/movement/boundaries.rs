//! String-based movement boundary detection, for command line/search mode.

use super::classify::{classify_char, CharClass};

/// Find the next word boundary position going forward from `start`
/// (a character index into `text`).
pub fn next_word(text: &str, start: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    if start >= len {
        return len;
    }

    let mut pos = start;
    let start_class = classify_char(chars[pos]);

    // 1. Skip current word category
    while pos < len && classify_char(chars[pos]) == start_class {
        pos += 1;
    }

    // 2. Skip whitespace if we weren't already on whitespace
    if start_class != CharClass::Whitespace {
        while pos < len && classify_char(chars[pos]) == CharClass::Whitespace {
            pos += 1;
        }
    }

    pos
}

/// Find the previous word boundary position going backward from `start`
/// (a character index into `text`).
pub fn prev_word(text: &str, start: usize) -> usize {
    if start == 0 {
        return 0;
    }

    let chars: Vec<char> = text.chars().collect();
    let mut pos = start - 1;

    // 1. Skip whitespace backwards
    while pos > 0 && classify_char(chars[pos]) == CharClass::Whitespace {
        pos -= 1;
    }

    // 2. Find start of current category
    let target_class = classify_char(chars[pos]);
    if target_class == CharClass::Whitespace {
        // Still whitespace? Means start is whitespace
        return pos + 1;
    }

    while pos > 0 {
        if classify_char(chars[pos - 1]) == target_class {
            pos -= 1;
        } else {
            break;
        }
    }

    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_word_basic() {
        assert_eq!(next_word("hello world", 0), 6);
        assert_eq!(next_word("foo->bar", 0), 3);
    }

    #[test]
    fn prev_word_basic() {
        assert_eq!(prev_word("hello world", 11), 6);
        assert_eq!(prev_word("foo->bar", 8), 5);
    }
}
