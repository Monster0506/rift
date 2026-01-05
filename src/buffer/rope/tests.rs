use super::*;
use crate::character::Character;

fn chars(s: &str) -> Vec<Character> {
    s.chars().map(Character::from).collect()
}

#[test]
fn test_empty_buffer() {
    let pt = PieceTable::new(Vec::new());
    assert!(pt.is_empty());
    assert_eq!(pt.len(), 0);
    assert_eq!(pt.get_line_count(), 1);
    assert_eq!(pt.to_string(), "");
}

#[test]
fn test_insert_basic() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("Hello"));
    assert_eq!(pt.to_string(), "Hello");
    assert_eq!(pt.len(), 5);

    pt.insert(5, &chars(" World"));
    assert_eq!(pt.to_string(), "Hello World");
    assert_eq!(pt.len(), 11);

    pt.insert(0, &chars("Start "));
    assert_eq!(pt.to_string(), "Start Hello World");
}

#[test]
fn test_insert_middle() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("AC"));
    pt.insert(1, &chars("B"));
    assert_eq!(pt.to_string(), "ABC");
}

#[test]
fn test_delete_basic() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("Hello World"));

    // Delete " World"
    pt.delete(5..11);
    assert_eq!(pt.to_string(), "Hello");

    // Delete "He"
    pt.delete(0..2);
    assert_eq!(pt.to_string(), "llo");
}

#[test]
fn test_delete_middle() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("ABCDE"));
    pt.delete(1..4); // Delete BCD
    assert_eq!(pt.to_string(), "AE");
}

#[test]
fn test_lines_basic() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("Line 1\nLine 2\nLine 3"));

    assert_eq!(pt.get_line_count(), 3);

    assert_eq!(pt.line_start_offset(0), 0);
    assert_eq!(pt.line_start_offset(1), 7); // "Line 1\n" is 7 chars
    assert_eq!(pt.line_start_offset(2), 14); // "Line 2\n" is 7 chars

    assert_eq!(pt.line_at_char(0), 0);
    assert_eq!(pt.line_at_char(6), 0); // '1'
    assert_eq!(pt.line_at_char(7), 1); // 'L' of Line 2
    assert_eq!(pt.line_at_char(13), 1);
    assert_eq!(pt.line_at_char(14), 2);
}

#[test]
fn test_lines_incremental() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("A"));
    assert_eq!(pt.get_line_count(), 1);

    pt.insert(1, &chars("\nB"));
    assert_eq!(pt.to_string(), "A\nB");
    assert_eq!(pt.get_line_count(), 2);

    pt.insert(0, &chars("\n"));
    assert_eq!(pt.to_string(), "\nA\nB");
    assert_eq!(pt.get_line_count(), 3);

    pt.delete(0..1);
    assert_eq!(pt.to_string(), "A\nB");
    assert_eq!(pt.get_line_count(), 2);
}

#[test]
fn test_char_access() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("0123456789"));

    assert_eq!(pt.char_at(0), Character::from('0'));
    assert_eq!(pt.char_at(5), Character::from('5'));
    assert_eq!(pt.char_at(9), Character::from('9'));

    let range = pt.bytes_range(3..7); // Should this be chars_range?
                                      // bytes_range returns Vec<u8> (via UTF-8 encoding of chars)
    assert_eq!(range, b"3456");
}

#[test]
fn test_complex_edits() {
    let mut pt = PieceTable::new(Vec::new());
    // 1. Insert initial text
    pt.insert(0, &chars("The quick brown fox"));

    // 2. Insert " jumps over the lazy dog"
    pt.insert(19, &chars(" jumps over the lazy dog"));
    assert_eq!(
        pt.to_string(),
        "The quick brown fox jumps over the lazy dog"
    );

    // 3. Delete "quick "
    pt.delete(4..10);
    assert_eq!(pt.to_string(), "The brown fox jumps over the lazy dog");

    // 4. Insert "red " before "fox"
    // "The brown " is 10 chars.
    pt.insert(10, &chars("red "));
    assert_eq!(pt.to_string(), "The brown red fox jumps over the lazy dog");

    // 5. Split lines
    // "The brown red fox" -> 17 chars
    pt.insert(17, &chars("\n"));
    assert_eq!(pt.get_line_count(), 2);
    assert_eq!(pt.line_start_offset(1), 18);
}

#[test]
fn test_original_buffer_usage() {
    let original = chars("Original Text");
    let mut pt = PieceTable::new(original);

    assert_eq!(pt.to_string(), "Original Text");

    pt.insert(0, &chars("Start "));
    assert_eq!(pt.to_string(), "Start Original Text");

    pt.delete(6..14); // Delete "Original"
    assert_eq!(pt.to_string(), "Start  Text");
}

#[test]
fn test_line_at_char_edge_cases() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("\n\n"));
    assert_eq!(pt.get_line_count(), 3);

    assert_eq!(pt.line_at_char(0), 0); // First newline
    assert_eq!(pt.line_at_char(1), 1); // Second newline
    assert_eq!(pt.line_at_char(2), 2); // End of buffer (empty line 3)
}

#[test]
fn test_delete_across_pieces() {
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &chars("Part1"));
    pt.insert(5, &chars("Part2"));
    pt.insert(10, &chars("Part3"));
    // "Part1Part2Part3"

    // Delete "t1Part2Pa" (indices 3 to 12)
    pt.delete(3..12);
    assert_eq!(pt.to_string(), "Parrt3");
}
