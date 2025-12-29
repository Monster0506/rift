use super::*;

#[test]
fn test_new_line_index() {
    let idx = LineIndex::new();
    assert_eq!(idx.line_count(), 1);
    assert_eq!(idx.len(), 0);
    assert!(idx.is_empty());
}

#[test]
fn test_insert_basic() {
    let mut idx = LineIndex::new();
    idx.insert(0, b"Hello");
    assert_eq!(idx.len(), 5);
    assert_eq!(idx.line_count(), 1);
    assert_eq!(idx.get_start(0), Some(0));
}

#[test]
fn test_insert_newlines() {
    let mut idx = LineIndex::new();
    idx.insert(0, b"Line 1\nLine 2");
    assert_eq!(idx.line_count(), 2);
    assert_eq!(idx.get_start(0), Some(0));
    assert_eq!(idx.get_start(1), Some(7)); // "Line 1\n" is 7 bytes
}

#[test]
fn test_get_end() {
    let mut idx = LineIndex::new();
    // "Line 1\nLine 2"
    idx.insert(0, b"Line 1\nLine 2");
    let total_len = idx.len();

    // Line 0: "Line 1" (len 6). Newline at 6.
    // get_end returns position of newline (exclusive end of content)
    assert_eq!(idx.get_end(0, total_len), Some(6));

    // Line 1: "Line 2" (len 6). End of buffer at 13.
    assert_eq!(idx.get_end(1, total_len), Some(13));

    assert_eq!(idx.get_end(2, total_len), None);
}

#[test]
fn test_get_line_at() {
    let mut idx = LineIndex::new();
    idx.insert(0, b"A\nB\nC");
    // 0: 'A', 1: '\n' -> Line 0
    // 2: 'B', 3: '\n' -> Line 1
    // 4: 'C'          -> Line 2

    assert_eq!(idx.get_line_at(0), 0);
    assert_eq!(idx.get_line_at(1), 0); // Newline belongs to line 0
    assert_eq!(idx.get_line_at(2), 1);
    assert_eq!(idx.get_line_at(3), 1);
    assert_eq!(idx.get_line_at(4), 2);
}

#[test]
fn test_delete() {
    let mut idx = LineIndex::new();
    idx.insert(0, b"Line 1\nLine 2");
    // Delete "\nLine " (indices 6 to 11)
    // "Line 12"
    idx.delete(6, 6);

    assert_eq!(idx.line_count(), 1);
    let bytes = idx.bytes_range(0..idx.len());
    assert_eq!(bytes, b"Line 12");
}

#[test]
fn test_byte_access() {
    let mut idx = LineIndex::new();
    idx.insert(0, b"Hello");
    assert_eq!(idx.byte_at(0), b'H');
    assert_eq!(idx.byte_at(4), b'o');

    let range = idx.bytes_range(1..4);
    assert_eq!(range, b"ell");
}
