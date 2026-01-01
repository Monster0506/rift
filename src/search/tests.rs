use super::*;
use crate::buffer::api::BufferView;

// --- Mock Buffer Implementation ---

struct MockBuffer {
    lines: Vec<String>,
}

impl MockBuffer {
    fn new(lines: &[&str]) -> Self {
        Self {
            lines: lines.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl BufferView for MockBuffer {
    fn len(&self) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        // Assume implicit newline after every line for simplicity in tests
        self.lines.iter().map(|l| l.chars().count() + 1).sum()
    }

    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn line_start(&self, line: usize) -> usize {
        let mut start = 0;
        for i in 0..line {
            start += self.lines[i].chars().count() + 1; // +1 for implicit newline
        }
        start
    }

    fn line_bytes(&self, line: usize) -> impl Iterator<Item = &[u8]> + '_ {
        std::iter::once(self.lines[line].as_bytes())
    }

    fn slice(&self, _start: usize, _end: usize) -> impl Iterator<Item = &[u8]> + '_ {
        std::iter::empty()
    }

    fn revision(&self) -> u64 {
        0
    }
}

// --- Tests ---

#[test]
fn test_find_next_forward_simple() {
    let buffer = MockBuffer::new(&["hello world", "another line"]);

    // Search "world" from start
    let res = find_next(&buffer, 0, "world", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 6..11); // "world" is at index 6

    // Search "hello" from start
    let res = find_next(&buffer, 0, "hello", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 0..5);
}

#[test]
fn test_find_next_forward_next_line() {
    let buffer = MockBuffer::new(&["line one", "line two"]);

    // Search "two" from start of file
    let res = find_next(&buffer, 0, "two", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    // "line one" (8) + \n (1) = 9. "line two" starts at 9. "two" starts at 9+5 = 14.
    assert_eq!(m.range, 14..17);
}

#[test]
fn test_find_next_forward_wrap() {
    let buffer = MockBuffer::new(&["first", "second", "third"]);

    // Cursor at "second" (start of line 1), search for "first"
    // "first" (5) + \n (1) = 6.
    let start_pos = 6;
    let res = find_next(&buffer, start_pos, "first", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 0..5);
}

#[test]
fn test_find_next_backward_simple() {
    let buffer = MockBuffer::new(&["hello world"]);

    // Cursor at end, search "hello"
    let res = find_next(&buffer, 10, "hello", SearchDirection::Backward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 0..5);
}

#[test]
fn test_find_next_backward_wrap() {
    let buffer = MockBuffer::new(&["first", "second"]);

    // Cursor at "first", search "second"
    // Should wrap to end of file and find "second"
    let res = find_next(&buffer, 0, "second", SearchDirection::Backward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    // "first" (5) + \n (1) = 6. "second" starts at 6.
    assert_eq!(m.range, 6..12);
}

#[test]
fn test_find_next_backward_same_line() {
    let buffer = MockBuffer::new(&["foo bar baz"]);

    // Cursor at "baz" (8), search "bar" (4..7)
    let res = find_next(&buffer, 8, "bar", SearchDirection::Backward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 4..7);
}

#[test]
fn test_unicode_offsets() {
    // "Héllo" -> 'H' (0), 'é' (1), 'l' (2), 'l' (3), 'o' (4)
    // Byte offsets: 'H' (0), 'é' (1..3), 'l' (3), ...
    let buffer = MockBuffer::new(&["Héllo world"]);

    // Search "world"
    // "Héllo " is 7 bytes ('é' is 2 bytes). "world" starts at 7.
    let res = find_next(&buffer, 0, "world", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 7..12);

    // Search "é"
    let res = find_next(&buffer, 0, "é", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 1..3);
}

#[test]
fn test_multiline_search() {
    let buffer = MockBuffer::new(&["line one", "line two"]);

    // Search for pattern spanning newline
    // "one\nline"
    // Note: MockBuffer adds implicit \n
    let res = find_next(&buffer, 0, "one\\nline", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    // "line one" -> "one" starts at 5.
    // Match: "one" (3) + "\n" (1) + "line" (4) = 8 chars
    // Range: 5..13
    assert_eq!(m.range, 5..13);
}

#[test]
fn test_multiline_wrap() {
    let buffer = MockBuffer::new(&["A", "B", "C"]);
    // Search "A\nB" from C
    let res = find_next(&buffer, 4, "A\\nB", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 0..3); // "A" (1) + \n (1) + "B" (1) = 3 chars
}

#[test]
fn test_case_sensitivity() {
    let buffer = MockBuffer::new(&["Hello"]);

    // Smart case: "hello" matches "Hello"
    let res = find_next(&buffer, 0, "hello", SearchDirection::Forward).unwrap();
    assert!(res.is_some());

    // Explicit case insensitive: "HELLO/i" matches "Hello"
    let res = find_next(&buffer, 0, "HELLO/i", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
}

#[test]
fn test_regex_anchors() {
    let buffer = MockBuffer::new(&["foo bar", "baz qux"]);

    // ^ matches start of line in line-by-line mode?
    // regex crate: ^ matches start of text.
    // In line-by-line mode, we feed one line at a time. So ^ matches start of line.

    let res = find_next(&buffer, 0, "^baz", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    // "foo bar" (7) + \n (1) = 8. "baz" starts at 8.
    assert_eq!(m.range, 8..11);

    // $ matches end of line
    let res = find_next(&buffer, 0, "bar$", SearchDirection::Forward).unwrap();
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 4..7);
}

#[test]
fn test_no_match() {
    let buffer = MockBuffer::new(&["hello world"]);
    let res = find_next(&buffer, 0, "xyz", SearchDirection::Forward).unwrap();
    assert!(res.is_none());
}

#[test]
fn test_empty_buffer() {
    let buffer = MockBuffer::new(&[]);
    let res = find_next(&buffer, 0, "abc", SearchDirection::Forward).unwrap();
    assert!(res.is_none());
}
