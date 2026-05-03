
use super::*;

#[test]
fn test_create_directory_entry_assigns_unique_ids() {
    let mut store = AnnotationStore::new();
    let id1 = store.create_directory_entry(1, 10);
    let id2 = store.create_directory_entry(2, 20);
    assert_ne!(id1, id2);
    assert!(id1 > 0);
    assert!(id2 > id1);
}

#[test]
fn test_directory_entry_id_at_line_hit() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(3, 42);
    assert_eq!(store.directory_entry_id_at_line(3), Some(42));
}

#[test]
fn test_directory_entry_id_at_line_miss() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(3, 42);
    assert_eq!(store.directory_entry_id_at_line(0), None);
    assert_eq!(store.directory_entry_id_at_line(2), None);
    assert_eq!(store.directory_entry_id_at_line(4), None);
}

#[test]
fn test_directory_entries_by_line_sorted() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(5, 50);
    store.create_directory_entry(1, 10);
    store.create_directory_entry(3, 30);
    let entries = store.directory_entries_by_line();
    assert_eq!(entries, vec![(1, 10), (3, 30), (5, 50)]);
}

#[test]
fn test_on_lines_deleted_removes_in_range() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.create_directory_entry(3, 3);
    store.on_lines_deleted(2, 1);
    // Entry 2 deleted; entry 3 shifts to line 2
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (2, 3)]);
}

#[test]
fn test_on_lines_deleted_multi_count() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.create_directory_entry(3, 3);
    store.create_directory_entry(4, 4);
    store.on_lines_deleted(2, 2);
    // Entries 2 and 3 deleted; entry 4 shifts to line 2
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (2, 4)]);
}

#[test]
fn test_on_lines_deleted_before_range_unchanged() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(3, 3);
    store.on_lines_deleted(2, 1);
    let entries = store.directory_entries_by_line();
    assert_eq!(entries[0], (1, 1), "line 1 should not shift");
    assert_eq!(entries[1], (2, 3), "line 3 should shift to line 2");
}

#[test]
fn test_on_line_inserted_shifts_at_and_after() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.on_line_inserted(2);
    // Line 2 shifts to 3; line 1 unchanged
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (3, 2)]);
}

#[test]
fn test_on_line_inserted_at_zero_shifts_all() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(0, 1);
    store.create_directory_entry(1, 2);
    store.on_line_inserted(0);
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (2, 2)]);
}

#[test]
fn test_on_line_inserted_before_does_not_shift_earlier() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(3, 3);
    store.on_line_inserted(2);
    // Line 1 unchanged; line 3 shifts to 4
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (4, 3)]);
}

#[test]
fn test_clear_removes_all() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.clear();
    assert!(store.directory_entries_by_line().is_empty());
    assert_eq!(store.directory_entry_id_at_line(1), None);
}

#[test]
fn test_on_lines_deleted_zero_count_is_noop() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.on_lines_deleted(1, 0);
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1)]);
}

#[test]
fn test_multiple_annotations_same_kind_different_lines() {
    let mut store = AnnotationStore::new();
    for i in 1..=5u16 {
        store.create_directory_entry(i as usize, i);
    }
    let entries = store.directory_entries_by_line();
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[0], (1, 1));
    assert_eq!(entries[4], (5, 5));
}
