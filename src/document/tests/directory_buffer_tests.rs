use super::common::*;
use super::*;
use crate::color::Color;

#[test]
fn test_populate_directory_first_line_is_parent_nav() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(vec![]);
    let text = doc.buffer.to_string();
    assert_eq!(text.lines().next().unwrap(), "../");
}

#[test]
fn test_populate_directory_file_entry_no_slash() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("hello.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    let text = doc.buffer.to_string();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[1], "hello.txt");
}

#[test]
fn test_populate_directory_dir_entry_has_trailing_slash() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("subdir", true)], "/tmp");
    doc.populate_directory_buffer(entries);
    let text = doc.buffer.to_string();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[1], "subdir/");
}

#[test]
fn test_populate_directory_no_ids_in_output() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.rs", false), ("src", true)], "/tmp");
    doc.populate_directory_buffer(entries);
    let text = doc.buffer.to_string();
    for line in text.lines() {
        let trimmed = line.trim_start();
        assert!(
            !trimmed.starts_with(|c: char| c.is_ascii_digit()),
            "line should not start with digit: {:?}",
            line
        );
    }
}

#[test]
fn test_populate_directory_multiple_entries_order() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.txt", false), ("b.txt", false), ("c", true)], "/tmp");
    doc.populate_directory_buffer(entries);
    let text = doc.buffer.to_string();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "../");
    assert_eq!(lines[1], "a.txt");
    assert_eq!(lines[2], "b.txt");
    assert_eq!(lines[3], "c/");
}

#[test]
fn test_populate_directory_no_trailing_newline() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    let text = doc.buffer.to_string();
    assert!(!text.ends_with('\n'));
}

#[test]
fn test_populate_directory_empty_dir_just_parent_nav() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(vec![]);
    let text = doc.buffer.to_string();
    assert_eq!(text, "../");
}

#[test]
fn test_populate_directory_updates_entries_snapshot() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    assert_eq!(doc.directory_entries().map(<[DirEntry]>::len), Some(1));
}

#[test]
fn test_populate_directory_marks_saved() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(vec![]);
    assert!(!doc.is_dirty());
}

#[test]
fn test_populate_directory_highlights_non_empty() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    assert!(!doc.custom_highlights.is_empty());
}

#[test]
fn test_populate_directory_parent_nav_is_blue() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(vec![]);
    let parent_range = 0..3; // "../" is 3 bytes
    let covered = doc.custom_highlights.iter().any(|(r, c)| {
        r.start <= parent_range.start && r.end >= parent_range.end && *c == Color::Blue
    });
    assert!(
        covered,
        "parent nav line should be Blue: {:?}",
        doc.custom_highlights
    );
}

#[test]
fn test_populate_directory_file_entry_is_white() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("readme.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    let _text = doc.buffer.to_string();
    let start = 4;
    let end = start + "readme.txt".len();
    let covered = doc
        .custom_highlights
        .iter()
        .any(|(r, c)| r.start <= start && r.end >= end && *c == Color::White);
    assert!(
        covered,
        "file entry should be White: {:?}",
        doc.custom_highlights
    );
}

#[test]
fn test_populate_directory_dir_entry_is_blue() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("subdir", true)], "/tmp");
    doc.populate_directory_buffer(entries);
    let start = 4;
    let end = start + "subdir/".len();
    let covered = doc
        .custom_highlights
        .iter()
        .any(|(r, c)| r.start <= start && r.end >= end && *c == Color::Blue);
    assert!(
        covered,
        "dir entry should be Blue: {:?}",
        doc.custom_highlights
    );
}

#[test]
fn test_populate_directory_clears_old_highlights_on_repopulate() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.txt", false), ("b.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    let first_count = doc.custom_highlights.len();

    let entries2 = make_dir_entries(&[("a.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries2);
    let second_count = doc.custom_highlights.len();

    assert!(
        second_count < first_count || second_count > 0,
        "highlights should be rebuilt on repopulate"
    );
}

#[test]
fn test_populate_directory_no_overlapping_highlight_ranges() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.txt", false), ("b", true), ("c.rs", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    let highlights = &doc.custom_highlights;
    for i in 0..highlights.len().saturating_sub(1) {
        assert!(
            highlights[i].0.end <= highlights[i + 1].0.start,
            "highlight ranges must not overlap: {:?} vs {:?}",
            highlights[i],
            highlights[i + 1]
        );
    }
}

#[test]
fn test_dir_entry_fields() {
    let entry = DirEntry {
        path: PathBuf::from("/tmp/file.txt"),
        is_dir: false,
        id: 0,
    };
    assert!(!entry.is_dir);
    assert_eq!(entry.path, PathBuf::from("/tmp/file.txt"));
    assert_eq!(entry.id, 0);
}

#[test]
fn test_dir_entry_directory() {
    let entry = DirEntry {
        path: PathBuf::from("/tmp/subdir"),
        is_dir: true,
        id: 0,
    };
    assert!(entry.is_dir);
}

#[test]
fn test_populate_directory_increments_revision() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let rev0 = doc.buffer.revision;
    doc.populate_directory_buffer(vec![]);
    assert!(
        doc.buffer.revision > rev0,
        "revision should increment after populate"
    );
}

#[test]
fn test_populate_directory_creates_annotations_in_store() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.txt", false), ("b.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    assert_eq!(doc.annotations.directory_entry_id_at_line(0), None);
    assert_eq!(doc.annotations.directory_entry_id_at_line(1), Some(1));
    assert_eq!(doc.annotations.directory_entry_id_at_line(2), Some(2));
}

#[test]
fn test_populate_directory_entries_are_interactive_at_their_line() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(make_dir_entries(&[("a.txt", false), ("sub", true)], "/tmp"));
    let a = doc.annotations.interactive_at_line(1).unwrap();
    assert_eq!(a.kind.as_str(), "fs.entry");
    assert_eq!(a.default_action().unwrap().verb, "activate");
    assert!(doc.annotations.interactive_at_line(2).is_some());
    assert!(doc.annotations.interactive_at_line(0).is_none());
}

#[test]
fn test_populate_directory_stores_name_and_is_dir_in_payload() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.txt", false), ("sub", true)], "/tmp");
    doc.populate_directory_buffer(entries);
    assert_eq!(
        doc.annotations.directory_entry_info_at_line(1),
        Some(("a.txt".to_string(), false))
    );
    assert_eq!(
        doc.annotations.directory_entry_info_at_line(2),
        Some(("sub".to_string(), true))
    );
    assert_eq!(doc.annotations.directory_entry_info_at_line(0), None);
}

#[test]
fn test_populate_directory_clears_previous_annotations_on_repopulate() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(make_dir_entries(
        &[("a.txt", false), ("b.txt", false)],
        "/tmp",
    ));
    doc.populate_directory_buffer(make_dir_entries(&[("c.txt", false)], "/tmp"));
    assert_eq!(doc.annotations.directory_entry_id_at_line(1), Some(1));
    assert_eq!(doc.annotations.directory_entry_id_at_line(2), None);
}

#[test]
fn test_annotation_store_line_insert_shifts_entries() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(make_dir_entries(
        &[("a.txt", false), ("b.txt", false)],
        "/tmp",
    ));
    doc.annotations.on_line_inserted(2);
    assert_eq!(doc.annotations.directory_entry_id_at_line(1), Some(1)); // a.txt unchanged
    assert_eq!(doc.annotations.directory_entry_id_at_line(2), None); // gap line
    assert_eq!(doc.annotations.directory_entry_id_at_line(3), Some(2)); // b.txt shifted
}

#[test]
fn test_annotation_store_line_delete_removes_entry() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(make_dir_entries(
        &[("a.txt", false), ("b.txt", false), ("c.txt", false)],
        "/tmp",
    ));
    doc.annotations.on_lines_deleted(2, 1, 2);
    assert_eq!(doc.annotations.directory_entry_id_at_line(1), Some(1)); // a.txt unchanged
    assert_eq!(doc.annotations.directory_entry_id_at_line(2), Some(3)); // c.txt shifted up
    assert_eq!(doc.annotations.directory_entry_id_at_line(3), None);
}

#[test]
fn test_buffer_text_has_no_annotation_prefix_after_populate() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(make_dir_entries(&[("readme.md", false)], "/tmp"));
    let text = doc.buffer.to_string();
    for line in text.lines() {
        let b = line.as_bytes();
        let is_id_prefix = b.len() >= 5
            && b[0] == b'/'
            && b[1].is_ascii_digit()
            && b[2].is_ascii_digit()
            && b[3].is_ascii_digit()
            && b[4] == b' ';
        assert!(
            !is_id_prefix,
            "buffer line must not start with /NNN prefix: {:?}",
            line
        );
    }
}
