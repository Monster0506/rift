use super::interval_tree::IntervalTree;
use crate::buffer::TextBuffer;
use super::interval_tree::IntervalTree;

#[test]
fn test_text_provider_chunks() {
    let mut buffer = TextBuffer::new(100).unwrap();
    buffer.insert_str("line1\nline2\nline3").unwrap();

    // Test collecting chunks from the provider - replaced by to_string check
    // since chunks_in_range was removed
    assert_eq!(buffer.to_string(), "line1\nline2\nline3");
}

#[test]
fn test_syntax_new_placeholder() {
    // Basic test to ensure TextBuffer is usable
    let buffer = TextBuffer::new(10).unwrap();
    assert_eq!(buffer.len(), 0);
}

// =============================================================================
// IntervalTree Tests
// =============================================================================

#[test]
fn test_interval_tree_basic() {
    let items = vec![(0..10, 1), (5..15, 2), (20..30, 3)];

    let tree = IntervalTree::new(items);

    let res = tree.query(0..5);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].1, 1);

    let res = tree.query(5..10);
    assert_eq!(res.len(), 2);

    let res = tree.query(16..19);
    assert_eq!(res.len(), 0);

    let res = tree.query(25..26);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].1, 3);
}

#[test]
fn test_interval_tree_nested() {
    let items = vec![(0..100, 1), (10..20, 2), (50..60, 3)];

    let tree = IntervalTree::new(items);

    let res = tree.query(15..16);
    assert_eq!(res.len(), 2);

    let res = tree.query(55..56);
    assert_eq!(res.len(), 2);

    let res = tree.query(5..6);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].1, 1);
}

#[test]
fn test_interval_tree_empty() {
    let tree: IntervalTree<i32> = IntervalTree::new(vec![]);
    assert!(tree.query(0..10).is_empty());
}

#[test]
fn test_interval_tree_sorted_query() {
    // Tree structure: Root (5..15), Left (0..10), Right (20..30)
    let items = vec![(0..10, 1), (5..15, 2), (20..30, 3)];

    let tree = IntervalTree::new(items);

    // Query (0..30) should return all, sorted by start
    let res = tree.query(0..30);
    assert_eq!(res.len(), 3);
    assert_eq!(res[0].1, 1); // 0..10 sorted first
    assert_eq!(res[1].1, 2); // 5..15
    assert_eq!(res[2].1, 3); // 20..30
}
