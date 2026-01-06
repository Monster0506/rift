//! String navigation utilities
//! Provides common navigation operations for string-based input (command line, search, etc.)

/// Find the cursor position after moving to the previous word start
///
/// # Arguments
/// * `content` - The string to navigate in
/// * `cursor` - Current cursor position (character index, not byte index)
///
/// # Returns
/// New cursor position
pub fn previous_word_start(content: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }

    let chars: Vec<char> = content.chars().collect();
    let mut idx = cursor;

    // 1. Skip preceding whitespace if any
    while idx > 0 && idx <= chars.len() && chars[idx - 1].is_whitespace() {
        idx -= 1;
    }

    // 2. Skip word characters
    while idx > 0 && idx <= chars.len() && !chars[idx - 1].is_whitespace() {
        idx -= 1;
    }

    idx
}

/// Find the cursor position after moving to the next word start
///
/// # Arguments
/// * `content` - The string to navigate in
/// * `cursor` - Current cursor position (character index, not byte index)
///
/// # Returns
/// New cursor position
pub fn next_word_start(content: &str, cursor: usize) -> usize {
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut idx = cursor;

    if idx >= len {
        return len;
    }

    // 1. Skip current word characters
    while idx < len && !chars[idx].is_whitespace() {
        idx += 1;
    }

    // 2. Skip subsequent whitespace
    while idx < len && chars[idx].is_whitespace() {
        idx += 1;
    }

    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_previous_word_start() {
        let text = "hello world";

        // From end
        assert_eq!(previous_word_start(text, 11), 6);
        // From middle of word
        assert_eq!(previous_word_start(text, 8), 6);
        // From start of word
        assert_eq!(previous_word_start(text, 6), 0);
        // From beginning
        assert_eq!(previous_word_start(text, 0), 0);
    }

    #[test]
    fn test_next_word_start() {
        let text = "hello world";

        // From start
        assert_eq!(next_word_start(text, 0), 6);
        // From middle of word
        assert_eq!(next_word_start(text, 3), 6);
        // From end
        assert_eq!(next_word_start(text, 11), 11);
    }

    #[test]
    fn test_multiple_spaces() {
        let text = "hello    world";
        assert_eq!(previous_word_start(text, 13), 9);
        assert_eq!(next_word_start(text, 0), 9);
    }
}
