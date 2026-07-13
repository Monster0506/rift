use super::IntervalTree;

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

#[test]
fn test_shift_for_edit_insertion() {
    // Insert 5 bytes at offset 10: range before the edit is untouched, range
    // after shifts by +5, range straddling the edit is dropped.
    let items = vec![(0..5, 1), (20..30, 2), (8..12, 3)];
    let tree = IntervalTree::new(items);

    let shifted = tree.shift_for_edit(10, 10, 15);
    let mut shifted = shifted;
    shifted.sort_by_key(|(r, _)| r.start);

    assert_eq!(shifted, vec![(0..5, 1), (25..35, 2)]);
}

#[test]
fn test_shift_for_edit_deletion() {
    // Delete 4 bytes at offset 10 (old_end 14 -> new_end 10): range after
    // shifts by -4.
    let items = vec![(0..5, 1), (20..30, 2)];
    let tree = IntervalTree::new(items);

    let mut shifted = tree.shift_for_edit(10, 14, 10);
    shifted.sort_by_key(|(r, _)| r.start);

    assert_eq!(shifted, vec![(0..5, 1), (16..26, 2)]);
}

#[test]
fn test_shift_for_edit_touching_boundaries_are_kept() {
    // Ending exactly at the edit start is kept; starting exactly at its old
    // end is shifted (caller must still filter both against its fresh requery).
    let items = vec![(0..10, 1), (10..20, 2)];
    let tree = IntervalTree::new(items);

    let mut shifted = tree.shift_for_edit(10, 10, 12);
    shifted.sort_by_key(|(r, _)| r.start);

    assert_eq!(shifted, vec![(0..10, 1), (12..22, 2)]);
}

#[test]
fn test_shift_for_edit_preserves_order_of_same_range_duplicates() {
    // Two captures on the identical range (e.g. both @constructor and
    // @function) must keep their relative order through an unrelated edit.
    let items = vec![(30..40, 1), (50..52, 5), (50..52, 4), (60..70, 2)];
    let tree = IntervalTree::new(items);

    let shifted = tree.shift_for_edit(0, 0, 1);

    let dup: Vec<_> = shifted
        .into_iter()
        .filter(|(r, _)| *r == (51..53))
        .collect();
    assert_eq!(dup, vec![(51..53, 5), (51..53, 4)]);
}
