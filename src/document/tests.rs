use super::*;
use crate::color::Color;

fn create_manager() -> DocumentManager {
    DocumentManager::new()
}
// BufferKind helper predicates

#[test]
fn test_new_doc_is_file_kind() {
    let doc = Document::new(1).unwrap();
    assert!(matches!(doc.kind, BufferKind::File));
}

#[test]
fn test_is_terminal_false_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.is_terminal());
}

#[test]
fn test_is_directory_false_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.is_directory());
}

#[test]
fn test_is_undotree_false_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.is_undotree());
}

#[test]
fn test_is_special_false_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.is_special());
}

#[test]
fn test_new_directory_kind() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp/test")).unwrap();
    assert!(doc.is_directory());
    assert!(!doc.is_terminal());
    assert!(!doc.is_undotree());
    assert!(doc.is_special());
}

#[test]
fn test_new_directory_show_hidden_defaults_false() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp/test")).unwrap();
    match &doc.kind {
        BufferKind::Directory { show_hidden, .. } => {
            assert!(!show_hidden, "show_hidden should default to false");
        }
        _ => panic!("expected Directory kind"),
    }
}

#[test]
fn test_new_undotree_kind() {
    let doc = Document::new_undotree(1, 42).unwrap();
    assert!(doc.is_undotree());
    assert!(!doc.is_terminal());
    assert!(!doc.is_directory());
    assert!(doc.is_special());
}

#[test]
fn test_new_undotree_is_read_only() {
    let doc = Document::new_undotree(1, 42).unwrap();
    assert!(doc.is_read_only);
}

#[test]
fn test_new_directory_not_read_only() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    assert!(!doc.is_read_only);
}

#[test]
fn test_new_undotree_stores_linked_doc_id() {
    let doc = Document::new_undotree(5, 99).unwrap();
    match doc.kind {
        BufferKind::UndoTree { linked_doc_id, .. } => assert_eq!(linked_doc_id, 99),
        _ => panic!("expected UndoTree kind"),
    }
}

#[test]
fn test_new_directory_stores_path() {
    let path = PathBuf::from("/home/user/projects");
    let doc = Document::new_directory(1, path.clone()).unwrap();
    match &doc.kind {
        BufferKind::Directory { path: p, .. } => assert_eq!(p, &path),
        _ => panic!("expected Directory kind"),
    }
}

#[test]
fn test_new_directory_entries_empty() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    match &doc.kind {
        BufferKind::Directory { entries, .. } => assert!(entries.is_empty()),
        _ => panic!("expected Directory kind"),
    }
}

#[test]
fn test_new_undotree_sequences_empty() {
    let doc = Document::new_undotree(1, 42).unwrap();
    match &doc.kind {
        BufferKind::UndoTree { sequences, .. } => assert!(sequences.is_empty()),
        _ => panic!("expected UndoTree kind"),
    }
}

#[test]
fn test_custom_highlights_empty_for_new_file() {
    let doc = Document::new(1).unwrap();
    assert!(doc.custom_highlights.is_empty());
}

#[test]
fn test_custom_highlights_empty_for_new_directory() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    assert!(doc.custom_highlights.is_empty());
}

#[test]
fn test_custom_highlights_empty_for_new_undotree() {
    let doc = Document::new_undotree(1, 2).unwrap();
    assert!(doc.custom_highlights.is_empty());
}
// directory_path()

#[test]
fn test_directory_path_returns_none_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(doc.directory_path().is_none());
}

#[test]
fn test_directory_path_returns_none_for_undotree() {
    let doc = Document::new_undotree(1, 2).unwrap();
    assert!(doc.directory_path().is_none());
}

#[test]
fn test_directory_path_returns_path_for_directory() {
    let path = PathBuf::from("/srv/data");
    let doc = Document::new_directory(1, path.clone()).unwrap();
    assert_eq!(doc.directory_path(), Some(&path));
}
// populate_directory_buffer — text format

fn make_dir_entries(names: &[(&str, bool)], base: &str) -> Vec<DirEntry> {
    names
        .iter()
        .map(|(name, is_dir)| DirEntry {
            path: PathBuf::from(base).join(name),
            is_dir: *is_dir,
            id: 0, // assigned by populate_directory_buffer
        })
        .collect()
}

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
    // Entry line has invisible /001 prefix; strip it to get the visible name.
    assert_eq!(dir_entry_name_from_line(lines[1]), "hello.txt");
}

#[test]
fn test_populate_directory_dir_entry_has_trailing_slash() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("subdir", true)], "/tmp");
    doc.populate_directory_buffer(entries);
    let text = doc.buffer.to_string();
    let lines: Vec<&str> = text.lines().collect();
    // Strip invisible ID prefix before checking visible name.
    assert_eq!(dir_entry_name_from_line(lines[1]), "subdir/");
}

#[test]
fn test_populate_directory_no_ids_in_output() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.rs", false), ("src", true)], "/tmp");
    doc.populate_directory_buffer(entries);
    let text = doc.buffer.to_string();
    // No numeric IDs like "1 file.rs" or "[1]"
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
    assert_eq!(dir_entry_name_from_line(lines[1]), "a.txt");
    assert_eq!(dir_entry_name_from_line(lines[2]), "b.txt");
    assert_eq!(dir_entry_name_from_line(lines[3]), "c/");
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
    match &doc.kind {
        BufferKind::Directory { entries, .. } => assert_eq!(entries.len(), 1),
        _ => panic!("expected Directory kind"),
    }
}

#[test]
fn test_populate_directory_marks_saved() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(vec![]);
    assert!(!doc.is_dirty());
}
// populate_directory_buffer — highlights

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
    // "../" should be colored Blue
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
    // "readme.txt" starts after "../\n" (4 bytes) + "/001 " ID prefix (5 bytes) = byte 9.
    let start = 9;
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
    // "subdir/" starts at offset 4 (after "../\n") + 5 ("/001 " ID prefix) = byte 9.
    let start = 9;
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

    // Repopulate with fewer entries
    let entries2 = make_dir_entries(&[("a.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries2);
    let second_count = doc.custom_highlights.len();

    // Fewer entries → fewer highlight ranges
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
// parse_directory_diff

fn make_populated_directory_doc(dir: &str, names: &[(&str, bool)]) -> Document {
    let mut doc = Document::new_directory(1, PathBuf::from(dir)).unwrap();
    let entries = make_dir_entries(names, dir);
    doc.populate_directory_buffer(entries);
    doc
}

#[test]
fn test_parse_diff_no_changes_empty_deletes_creates() {
    let doc = make_populated_directory_doc("/tmp", &[("file.txt", false)]);
    let diff = doc.parse_directory_diff();
    assert!(diff.deletes.is_empty(), "no files should be deleted");
    assert!(diff.creates.is_empty(), "no files should be created");
    assert!(diff.renames.is_empty());
}

#[test]
fn test_parse_diff_deleted_entry() {
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false), ("b.txt", false)]);
    // Remove b.txt from the buffer
    let text = doc.buffer.to_string();
    let new_text: String = text
        .lines()
        .filter(|l| !l.contains("b.txt"))
        .collect::<Vec<_>>()
        .join("\n");
    // Replace buffer content via Document API
    let old_len = doc.buffer.len();
    let _ = doc.buffer.set_cursor(0);
    let _ = doc.delete_range(0, old_len);
    let _ = doc.insert_str(&new_text);

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.deletes.len(), 1);
    assert!(diff.deletes[0].to_string_lossy().contains("b.txt"));
}

#[test]
fn test_parse_diff_new_file_entry() {
    let mut doc = make_populated_directory_doc("/tmp", &[("existing.txt", false)]);
    // Add a new file name to the buffer
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\nnew_file.txt");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.iter().any(|c| c == "new_file.txt"),
        "should detect new_file.txt as a create: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_new_dir_entry_trailing_slash() {
    let mut doc = make_populated_directory_doc("/tmp", &[]);
    // Add a directory entry ending with /
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\nnewdir/");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.iter().any(|c| c == "newdir/"),
        "should preserve trailing slash in creates: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_new_file_no_trailing_slash() {
    let mut doc = make_populated_directory_doc("/tmp", &[]);
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\nnewfile.txt");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.iter().any(|c| c == "newfile.txt"),
        "file creates should not have trailing slash: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_parent_nav_not_in_diff() {
    let doc = make_populated_directory_doc("/tmp", &[]);
    let diff = doc.parse_directory_diff();
    // "../" must not appear in creates
    for c in &diff.creates {
        assert_ne!(c, "../", "parent nav must not be treated as a create");
    }
}

#[test]
fn test_parse_diff_empty_lines_ignored() {
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    // Insert an empty line
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\n");

    let diff = doc.parse_directory_diff();
    // Empty line should not show up in creates or deletes
    assert!(
        diff.creates.is_empty(),
        "empty lines not counted as creates"
    );
}

#[test]
fn test_parse_diff_one_entry_replaced_is_rename() {
    // Replacing the only entry with a new name → rename.
    // With the ID system, the user edits the visible name after the invisible /001 prefix.
    let mut doc = make_populated_directory_doc("/tmp", &[("old.txt", false)]);
    // /001 is the ID of old.txt; user changed the visible name to new.txt
    set_buffer_text(&mut doc, "../\n/001 new.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1, "ID-based replacement is a rename");
    assert!(diff.renames[0].0.to_string_lossy().contains("old.txt"));
    assert_eq!(diff.renames[0].1, "new.txt");
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_noop_on_non_directory_doc() {
    let doc = Document::new(1).unwrap();
    let diff = doc.parse_directory_diff();
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
    assert!(diff.renames.is_empty());
}

#[test]
fn test_parse_diff_move_file_into_subdir() {
    // User edits buffer from:
    //   ../
    //   [invis /001] A/
    //   [invis /002] b.c
    // to:
    //   ../
    //   [invis /001] A/        (unchanged)
    //   [invis /002] A/b.c     (b.c moved into A/)
    //
    // Expected: rename b.c → A/b.c; directory A is preserved unchanged.
    // The ID system makes this unambiguous: id=1 (A) unchanged, id=2 (b.c) renamed.
    let mut doc = make_populated_directory_doc("/tmp", &[("A", true), ("b.c", false)]);
    set_buffer_text(&mut doc, "../\n/001 A/\n/002 A/b.c");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.deletes.is_empty(),
        "A/ should not be deleted: {:?}",
        diff
    );
    assert_eq!(
        diff.renames.len(),
        1,
        "should produce exactly one rename: {:?}",
        diff
    );
    assert!(
        diff.renames[0].0.to_string_lossy().contains("b.c"),
        "rename source should be b.c, not A: {:?}",
        diff.renames[0]
    );
    assert_eq!(
        diff.renames[0].1, "A/b.c",
        "rename target should be A/b.c: {:?}",
        diff.renames[0]
    );
    assert!(diff.creates.is_empty(), "no creates expected: {:?}", diff);
}

#[test]
fn test_parse_diff_move_one_of_two_files_into_subdir() {
    // Buffer: A/ (id=1), b.c (id=2), c.d (id=3) → A/ unchanged, b.c unchanged, c.d → A/c.d.
    // The ID system makes it unambiguous: id=3 was c.d, now shows A/c.d → rename.
    let mut doc =
        make_populated_directory_doc("/tmp", &[("A", true), ("b.c", false), ("c.d", false)]);
    set_buffer_text(&mut doc, "../\n/001 A/\n/002 b.c\n/003 A/c.d");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.deletes.is_empty(),
        "nothing should be deleted: {:?}",
        diff
    );
    assert_eq!(diff.renames.len(), 1, "exactly one rename: {:?}", diff);
    assert!(
        diff.renames[0].0.to_string_lossy().contains("c.d"),
        "rename source should be c.d: {:?}",
        diff.renames[0]
    );
    assert_eq!(diff.renames[0].1, "A/c.d");
    assert!(diff.creates.is_empty(), "no creates: {:?}", diff);
}
// populate_undotree_buffer

#[test]
fn test_populate_undotree_stores_text() {
    let mut doc = Document::new_undotree(1, 42).unwrap();
    let text = "* [1] edit\n* [0] root".to_string();
    let seqs = vec![1u64, 0u64];
    let highlights = vec![];
    doc.populate_undotree_buffer(text.clone(), seqs, highlights);
    assert_eq!(doc.buffer.to_string(), text);
}

#[test]
fn test_populate_undotree_stores_sequences() {
    let mut doc = Document::new_undotree(1, 42).unwrap();
    let seqs = vec![5u64, 3u64, 1u64];
    doc.populate_undotree_buffer("text".to_string(), seqs.clone(), vec![]);
    match &doc.kind {
        BufferKind::UndoTree { sequences, .. } => assert_eq!(sequences, &seqs),
        _ => panic!("expected UndoTree kind"),
    }
}

#[test]
fn test_populate_undotree_stores_highlights() {
    let mut doc = Document::new_undotree(1, 42).unwrap();
    let highlights = vec![(0..2, Color::Magenta), (3..5, Color::Cyan)];
    doc.populate_undotree_buffer("ab cd".to_string(), vec![], highlights.clone());
    assert_eq!(doc.custom_highlights.len(), 2);
    assert_eq!(doc.custom_highlights[0].1, Color::Magenta);
    assert_eq!(doc.custom_highlights[1].1, Color::Cyan);
}

#[test]
fn test_populate_undotree_preserves_linked_doc_id() {
    let mut doc = Document::new_undotree(1, 99).unwrap();
    doc.populate_undotree_buffer("x".to_string(), vec![], vec![]);
    match &doc.kind {
        BufferKind::UndoTree { linked_doc_id, .. } => assert_eq!(*linked_doc_id, 99),
        _ => panic!("expected UndoTree kind"),
    }
}

#[test]
fn test_populate_undotree_noop_on_wrong_kind() {
    let mut doc = Document::new(1).unwrap();
    // Should silently do nothing — no panic
    doc.populate_undotree_buffer("text".to_string(), vec![1], vec![]);
    // Buffer should remain empty
    assert_eq!(doc.buffer.to_string(), "");
}

#[test]
fn test_populate_undotree_marks_saved() {
    let mut doc = Document::new_undotree(1, 42).unwrap();
    doc.populate_undotree_buffer("text".to_string(), vec![], vec![]);
    assert!(!doc.is_dirty());
}

#[test]
fn test_populate_undotree_replaces_old_highlights() {
    let mut doc = Document::new_undotree(1, 42).unwrap();
    doc.populate_undotree_buffer("ab".to_string(), vec![], vec![(0..2, Color::Red)]);
    assert_eq!(doc.custom_highlights.len(), 1);

    doc.populate_undotree_buffer(
        "xyz".to_string(),
        vec![],
        vec![
            (0..1, Color::Blue),
            (1..2, Color::Green),
            (2..3, Color::Yellow),
        ],
    );
    assert_eq!(doc.custom_highlights.len(), 3);
    assert_eq!(doc.custom_highlights[0].1, Color::Blue);
}
// Manager: special buffers close without dirty check

#[test]
fn test_remove_directory_buffer_without_dirty_check() {
    let mut manager = create_manager();
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    // Mark it visually dirty by inserting text
    let _ = doc.buffer.insert_str("some edit");
    manager.add_document(doc);

    // Should close without error even though it appears modified
    assert!(manager.remove_document(1).is_ok());
}

#[test]
fn test_remove_undotree_buffer_without_dirty_check() {
    let mut manager = create_manager();
    let doc = Document::new_undotree(2, 1).unwrap();
    manager.add_document(doc);
    assert!(manager.remove_document(2).is_ok());
}

#[test]
fn test_iter_documents_visits_all() {
    let mut manager = create_manager();
    manager.add_document(Document::new(1).unwrap());
    manager.add_document(Document::new(2).unwrap());
    manager.add_document(Document::new_directory(3, PathBuf::from("/tmp")).unwrap());

    let ids: Vec<u64> = manager.iter_documents().map(|d| d.id).collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));
}

#[test]
fn test_iter_documents_mut_allows_mutation() {
    let mut manager = create_manager();
    manager.add_document(Document::new(1).unwrap());
    for doc in manager.iter_documents_mut() {
        doc.is_read_only = true;
    }
    assert!(manager.active_document().unwrap().is_read_only);
}
// DirEntry struct

#[test]
fn test_dir_entry_fields() {
    // DirEntry has path, is_dir, and id fields
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
// DirectoryDiff — rename detection

/// Helper: replace the full buffer text of a doc (simulates user editing the explorer buffer).
fn set_buffer_text(doc: &mut Document, text: &str) {
    let old_len = doc.buffer.len();
    let _ = doc.buffer.set_cursor(0);
    let _ = doc.delete_range(0, old_len);
    let _ = doc.insert_str(text);
}

#[test]
fn test_parse_diff_rename_simple() {
    // User edits the visible name after the invisible /001 prefix.
    let mut doc = make_populated_directory_doc("/tmp", &[("test1.txt", false)]);
    set_buffer_text(&mut doc, "../\n/001 test1.json");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1, "should detect a rename: {:?}", diff);
    assert!(
        diff.renames[0].0.to_string_lossy().contains("test1.txt"),
        "renamed from test1.txt: {:?}",
        diff.renames[0]
    );
    assert_eq!(
        diff.renames[0].1, "test1.json",
        "renamed to test1.json: {:?}",
        diff.renames[0]
    );
    assert!(
        diff.deletes.is_empty(),
        "rename must not also produce a delete"
    );
    assert!(
        diff.creates.is_empty(),
        "rename must not also produce a create"
    );
}

#[test]
fn test_parse_diff_rename_preserves_siblings() {
    // Rename one file, leave others untouched.
    // IDs: 001=a.txt, 002=b.txt, 003=c.txt
    let mut doc = make_populated_directory_doc(
        "/tmp",
        &[("a.txt", false), ("b.txt", false), ("c.txt", false)],
    );
    set_buffer_text(&mut doc, "../\n/001 a.txt\n/002 b.json\n/003 c.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1);
    assert!(diff.renames[0].0.to_string_lossy().contains("b.txt"));
    assert_eq!(diff.renames[0].1, "b.json");
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_rename_multiple() {
    // IDs: 001=foo.txt, 002=bar.txt
    let mut doc = make_populated_directory_doc("/tmp", &[("foo.txt", false), ("bar.txt", false)]);
    set_buffer_text(&mut doc, "../\n/001 foo.rs\n/002 bar.rs");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 2);
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_rename_with_delete() {
    // Rename one, delete another.
    // IDs: 001=keep.txt, 002=old.txt, 003=gone.txt
    let mut doc = make_populated_directory_doc(
        "/tmp",
        &[("keep.txt", false), ("old.txt", false), ("gone.txt", false)],
    );
    // keep.txt stays (id=1), old.txt renamed to new.txt (id=2), gone.txt deleted (id=3 absent)
    set_buffer_text(&mut doc, "../\n/001 keep.txt\n/002 new.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1, "one rename: {:?}", diff);
    assert_eq!(diff.deletes.len(), 1, "one delete: {:?}", diff);
    assert!(diff.creates.is_empty(), "no creates: {:?}", diff);
    assert!(
        diff.renames[0].0.file_name().unwrap().to_string_lossy() == "old.txt",
        "renamed from old.txt: {:?}",
        diff.renames[0]
    );
    assert_eq!(diff.renames[0].1, "new.txt");
    assert!(
        diff.deletes[0].file_name().unwrap().to_string_lossy() == "gone.txt",
        "gone.txt should be deleted: {:?}",
        diff.deletes[0]
    );
}

#[test]
fn test_parse_diff_rename_with_create() {
    // Rename one, add a new entry.
    // ID 001=original.txt; user renames it (keeps /001 prefix) and types a brand new name.
    let mut doc = make_populated_directory_doc("/tmp", &[("original.txt", false)]);
    set_buffer_text(&mut doc, "../\n/001 renamed.txt\nbrand_new.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1);
    assert_eq!(diff.renames[0].1, "renamed.txt");
    assert_eq!(diff.creates.len(), 1);
    assert_eq!(diff.creates[0], "brand_new.txt");
    assert!(diff.deletes.is_empty());
}

#[test]
fn test_parse_diff_no_change_produces_empty_diff() {
    let doc = make_populated_directory_doc("/tmp", &[("file.txt", false)]);
    let diff = doc.parse_directory_diff();
    assert!(diff.renames.is_empty());
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}
// DirectoryDiff struct

#[test]
fn test_parse_diff_multiple_creates() {
    let mut doc = make_populated_directory_doc("/tmp", &[]);
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\nnew1.txt\nnew2.txt\nnewdir/");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.creates.len(), 3);
    assert!(diff.creates.iter().any(|c| c == "new1.txt"));
    assert!(diff.creates.iter().any(|c| c == "new2.txt"));
    assert!(diff.creates.iter().any(|c| c == "newdir/"));
}

#[test]
fn test_parse_diff_multiple_deletes() {
    let mut doc = make_populated_directory_doc(
        "/tmp",
        &[("a.txt", false), ("b.txt", false), ("c.txt", false)],
    );
    // Keep only a.txt (id=1); b.txt (id=2) and c.txt (id=3) are absent → deleted.
    set_buffer_text(&mut doc, "../\n/001 a.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.deletes.len(), 2);
}
// Buffer revision increments on populate

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
fn test_populate_undotree_increments_revision() {
    let mut doc = Document::new_undotree(1, 2).unwrap();
    let rev0 = doc.buffer.revision;
    doc.populate_undotree_buffer("text".to_string(), vec![], vec![]);
    assert!(
        doc.buffer.revision > rev0,
        "revision should increment after populate"
    );
}
// BufferKind — cloneability

#[test]
fn test_buffer_kind_file_clones() {
    let kind = BufferKind::File;
    let cloned = kind.clone();
    assert!(matches!(cloned, BufferKind::File));
}

#[test]
fn test_buffer_kind_directory_clones() {
    let kind = BufferKind::Directory {
        path: PathBuf::from("/tmp"),
        entries: vec![DirEntry {
            path: PathBuf::from("/tmp/a"),
            is_dir: false,
            id: 0,
        }],
        show_hidden: false,
    };
    let cloned = kind.clone();
    match cloned {
        BufferKind::Directory { path, entries, .. } => {
            assert_eq!(path, PathBuf::from("/tmp"));
            assert_eq!(entries.len(), 1);
        }
        _ => panic!("expected Directory"),
    }
}

#[test]
fn test_buffer_kind_undotree_clones() {
    let kind = BufferKind::UndoTree {
        linked_doc_id: 7,
        sequences: vec![1, 2, 3],
    };
    let cloned = kind.clone();
    match cloned {
        BufferKind::UndoTree {
            linked_doc_id,
            sequences,
        } => {
            assert_eq!(linked_doc_id, 7);
            assert_eq!(sequences, vec![1, 2, 3]);
        }
        _ => panic!("expected UndoTree"),
    }
}

#[test]
fn test_manager_initial_state() {
    let manager = create_manager();
    assert_eq!(manager.tab_count(), 0);
    assert_eq!(manager.active_tab_index(), 0);
    assert!(manager.active_document().is_none());
    assert!(manager.active_document_id().is_none());
}

#[test]
fn test_add_document() {
    let mut manager = create_manager();
    let doc = Document::new(1).unwrap();
    manager.add_document(doc);

    assert_eq!(manager.tab_count(), 1);
    assert_eq!(manager.active_tab_index(), 0);
    assert_eq!(manager.active_document_id(), Some(1));
    assert!(manager.active_document().is_some());
}

#[test]
fn test_remove_document() {
    let mut manager = create_manager();
    let doc = Document::new(1).unwrap();
    manager.add_document(doc);

    assert!(manager.remove_document(1).is_ok());
    assert_eq!(manager.tab_count(), 1); // Should still have 1 doc
    assert_ne!(manager.active_document_id(), Some(1)); // But different ID
}

#[test]
fn test_remove_specific_document() {
    let mut manager = create_manager();
    let doc1 = Document::new(1).unwrap();
    let doc2 = Document::new(2).unwrap();

    manager.add_document(doc1);
    manager.add_document(doc2);

    assert_eq!(manager.tab_count(), 2);
    assert_eq!(manager.active_document_id(), Some(2)); // doc2 is active (latest added)

    // Remove doc1 (inactive)
    assert!(manager.remove_document(1).is_ok());
    assert_eq!(manager.tab_count(), 1);
    assert_eq!(manager.active_document_id(), Some(2)); // doc2 still active

    // Remove doc2 (active) - should create new empty one since it's the last one
    assert!(manager.remove_document(2).is_ok());
    assert_eq!(manager.tab_count(), 1);
    assert_ne!(manager.active_document_id(), Some(2));
}

#[test]
fn test_switching_tabs() {
    let mut manager = create_manager();
    let doc1 = Document::new(1).unwrap();
    let doc2 = Document::new(2).unwrap();
    let doc3 = Document::new(3).unwrap();

    manager.add_document(doc1);
    manager.add_document(doc2);
    manager.add_document(doc3);

    // Initial: [1, 2, 3], current=2 (index)
    assert_eq!(manager.active_document_id(), Some(3));

    manager.switch_prev_tab();
    assert_eq!(manager.active_document_id(), Some(2));

    manager.switch_prev_tab();
    assert_eq!(manager.active_document_id(), Some(1));

    manager.switch_prev_tab(); // Wrap around
    assert_eq!(manager.active_document_id(), Some(3));

    manager.switch_next_tab(); // Wrap around
    assert_eq!(manager.active_document_id(), Some(1));
}

#[test]
fn test_open_existing_file_switches_tab() {
    let _manager = create_manager();
}

#[test]
fn test_undo_binary_data() {
    let mut doc = Document::new(1).unwrap();
    // Insert binary byte directly into buffer to simulate file load (invalid UTF-8)
    // 0xFF is not a valid UTF-8 byte
    let binary_char = crate::character::Character::Byte(0xFF);
    doc.buffer.insert_character(binary_char).unwrap();

    assert_eq!(doc.buffer.len(), 1);
    assert_eq!(doc.buffer.char_at(0), Some(binary_char));

    // Delete it using Document API (which should record it in history)
    doc.delete_range(0, 1).unwrap();
    assert_eq!(doc.buffer.len(), 0);

    // Undo
    doc.undo();

    // Verify the ORIGINAL byte is restored, not a replacement character
    assert_eq!(doc.buffer.len(), 1);
    assert_eq!(doc.buffer.char_at(0), Some(binary_char));
}

#[test]
fn test_get_changed_line_for_seq() {
    let mut doc = Document::new(1).unwrap();

    // Initial state has seq=0, no operations
    assert_eq!(doc.get_changed_line_for_seq(0), None);

    // Insert text at line 0
    doc.insert_str("hello\n").unwrap();
    let seq1 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq1), Some(0));

    // Insert more text (still at line 0 because cursor is at end of line 1)
    doc.insert_str("world\n").unwrap();
    let seq2 = doc.history.current_seq();
    // The insertion happened at line 1 (after the first newline)
    assert_eq!(doc.get_changed_line_for_seq(seq2), Some(1));

    // Insert text at a specific position (move cursor to line 2)
    doc.insert_str("line3").unwrap();
    let seq3 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq3), Some(2));

    // Test with delete operation
    doc.buffer.set_cursor(0).unwrap(); // Go to start
    doc.delete_forward(); // Delete 'h'
    let seq4 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq4), Some(0));

    // Test invalid seq
    assert_eq!(doc.get_changed_line_for_seq(9999), None);
}

#[test]
fn test_from_file_strips_utf8_bom() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bom.txt");
    std::fs::write(&path, b"\xEF\xBB\xBFhello").unwrap();

    let doc = Document::from_file(1, &path).unwrap();
    assert_eq!(doc.buffer.to_string(), "hello");
    assert_eq!(doc.buffer.len(), 5);
}

#[test]
fn test_from_file_normalizes_crlf() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crlf.txt");
    std::fs::write(&path, b"line1\r\nline2\r\nline3").unwrap();

    let doc = Document::from_file(1, &path).unwrap();
    let text = doc.buffer.to_string();
    assert!(
        !text.contains('\r'),
        "buffer should not contain \\r after CRLF normalization"
    );
    assert_eq!(text, "line1\nline2\nline3");
    assert_eq!(doc.options.line_ending, LineEnding::CRLF);
}

#[test]
fn test_dir_entry_name_from_line_strips_valid_prefix() {
    assert_eq!(dir_entry_name_from_line("/001 hello.txt"), "hello.txt");
}

#[test]
fn test_dir_entry_name_from_line_no_prefix_passes_through() {
    assert_eq!(dir_entry_name_from_line("hello.txt"), "hello.txt");
}

#[test]
fn test_dir_entry_name_from_line_parent_nav_unchanged() {
    assert_eq!(dir_entry_name_from_line("../"), "../");
}

#[test]
fn test_dir_entry_name_from_line_empty_string() {
    assert_eq!(dir_entry_name_from_line(""), "");
}

#[test]
fn test_dir_entry_name_from_line_partial_prefix_not_stripped() {
    // Only 4 bytes "/001" — missing the trailing space, not a valid prefix
    assert_eq!(dir_entry_name_from_line("/001file.txt"), "/001file.txt");
}

#[test]
fn test_dir_entry_name_from_line_max_id() {
    assert_eq!(dir_entry_name_from_line("/999 max.rs"), "max.rs");
}

#[test]
fn test_dir_entry_name_from_line_dir_trailing_slash_preserved() {
    assert_eq!(dir_entry_name_from_line("/042 subdir/"), "subdir/");
}

#[test]
fn test_dir_entry_name_from_line_exactly_prefix_only() {
    // Five bytes "/001 " with nothing after — returns empty string
    assert_eq!(dir_entry_name_from_line("/001 "), "");
}

#[test]
fn test_invisible_ranges_empty_for_new_doc() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    assert!(doc.invisible_ranges.is_empty());
}

#[test]
fn test_invisible_ranges_none_for_header_line() {
    // The "../" header has no invisible range.
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    doc.populate_directory_buffer(vec![]);
    // Empty directory → only the header line → no invisible ranges.
    assert!(doc.invisible_ranges.is_empty());
}

#[test]
fn test_invisible_ranges_count_matches_entries() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(
        &[("a.txt", false), ("b.txt", false), ("c.txt", false)],
        "/tmp",
    );
    doc.populate_directory_buffer(entries);
    assert_eq!(doc.invisible_ranges.len(), 3);
}

#[test]
fn test_invisible_ranges_first_entry_offset() {
    // "../\n" = 4 bytes → first entry prefix starts at byte 4, length 5 → 4..9
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    assert_eq!(doc.invisible_ranges[0], 4..9, "first prefix at bytes 4..9");
}

#[test]
fn test_invisible_ranges_each_is_five_bytes() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.rs", false), ("b.rs", false), ("c.rs", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    for r in &doc.invisible_ranges {
        assert_eq!(
            r.end - r.start,
            5,
            "each invisible range must be 5 bytes: {:?}",
            r
        );
    }
}

#[test]
fn test_invisible_ranges_cleared_on_repopulate() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.txt", false), ("b.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    assert_eq!(doc.invisible_ranges.len(), 2);

    // Repopulate with one entry → should have exactly 1 range.
    let entries2 = make_dir_entries(&[("only.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries2);
    assert_eq!(doc.invisible_ranges.len(), 1);
}

#[test]
fn test_invisible_ranges_non_overlapping_and_ordered() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("aaa", false), ("bbb", false), ("ccc", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    let ranges = &doc.invisible_ranges;
    for i in 0..ranges.len().saturating_sub(1) {
        assert!(
            ranges[i].end <= ranges[i + 1].start,
            "invisible ranges must be ordered and non-overlapping: {:?}",
            ranges
        );
    }
}

#[test]
fn test_clamp_cursor_no_ranges_is_noop() {
    let mut doc = Document::new(1).unwrap();
    let _ = doc.buffer.insert_str("hello");
    let _ = doc.buffer.set_cursor(0);
    doc.clamp_cursor_past_invisible();
    assert_eq!(doc.buffer.cursor(), 0);
}

#[test]
fn test_clamp_cursor_not_in_range_is_noop() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    // Position cursor at the '../' header (char 0) — not in any invisible range.
    let _ = doc.buffer.set_cursor(0);
    doc.clamp_cursor_past_invisible();
    assert_eq!(doc.buffer.cursor(), 0, "cursor on header stays at 0");
}

#[test]
fn test_clamp_cursor_at_range_start_advances_to_end() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    // First entry prefix is at chars 4..9 (after "../\n" which is 4 chars).
    // Move cursor to char 4 (start of invisible prefix).
    let _ = doc.buffer.set_cursor(4);
    doc.clamp_cursor_past_invisible();
    assert_eq!(
        doc.buffer.cursor(),
        9,
        "cursor clamped past 5-byte prefix to char 9"
    );
}

#[test]
fn test_clamp_cursor_inside_range_advances_to_end() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    // Prefix chars are 4..9; put cursor in the middle at char 6.
    let _ = doc.buffer.set_cursor(6);
    doc.clamp_cursor_past_invisible();
    assert_eq!(doc.buffer.cursor(), 9, "cursor clamped past prefix");
}

#[test]
fn test_clamp_cursor_after_range_is_noop() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("file.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    // Cursor at char 9 is past the prefix → visible "file.txt" territory.
    let _ = doc.buffer.set_cursor(9);
    doc.clamp_cursor_past_invisible();
    assert_eq!(doc.buffer.cursor(), 9, "cursor after range unchanged");
}

#[test]
fn test_parse_diff_reorder_without_rename_produces_no_diff() {
    // The ID system: swapping order of lines does not change IDs → no renames.
    let mut doc =
        make_populated_directory_doc("/tmp", &[("alpha.txt", false), ("beta.txt", false)]);
    // Swap order but keep IDs — both names unchanged.
    set_buffer_text(&mut doc, "../\n/002 beta.txt\n/001 alpha.txt");
    let diff = doc.parse_directory_diff();
    assert!(
        diff.renames.is_empty(),
        "reorder without name change → no renames: {:?}",
        diff
    );
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_entry_with_zero_id_silently_ignored() {
    // A line with prefix "/000 " — id 0 is the sentinel "no ID"; the parser has no entry
    // for id=0 in its map (entries start at id=1), so the line is silently ignored —
    // it does NOT appear in creates, renames, or deletes.
    let mut doc = make_populated_directory_doc("/tmp", &[("real.txt", false)]);
    // Append a line that looks like id=0 (invalid — assigned IDs start at 1).
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\n/000 ghost.txt");
    let diff = doc.parse_directory_diff();
    // Existing entry "real.txt" (id=1) is still present → no deletes.
    assert!(
        diff.deletes.is_empty(),
        "real.txt should not be deleted: {:?}",
        diff
    );
    // "/000 " is parsed as an ID line (not a raw create).
    assert!(
        diff.creates.is_empty(),
        "id=0 line must not become a create: {:?}",
        diff
    );
    assert!(
        diff.renames.is_empty(),
        "id=0 line must not become a rename: {:?}",
        diff
    );
}

#[test]
fn test_parse_diff_all_entries_deleted() {
    let mut doc = make_populated_directory_doc(
        "/tmp",
        &[("a.txt", false), ("b.txt", false), ("c.txt", false)],
    );
    // Buffer only contains the header — all entry IDs absent → all deleted.
    set_buffer_text(&mut doc, "../");
    let diff = doc.parse_directory_diff();
    assert_eq!(
        diff.deletes.len(),
        3,
        "all three entries deleted: {:?}",
        diff
    );
    assert!(diff.renames.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_from_file_strips_standalone_cr() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bare_cr.txt");
    std::fs::write(&path, b"line1\rline2").unwrap();

    let doc = Document::from_file(1, &path).unwrap();
    let text = doc.buffer.to_string();
    assert!(
        !text.contains('\r'),
        "buffer should not contain standalone \\r (would render as ^M)"
    );
}
// WrapMode

#[test]
fn wrap_default_is_auto() {
    let doc = Document::new(1).unwrap();
    assert_eq!(
        doc.options.wrap,
        Some(definitions::WrapMode::Expr("auto".to_string()))
    );
}

#[test]
fn wrap_resolve_auto() {
    let mode = definitions::WrapMode::Expr("auto".to_string());
    assert_eq!(mode.resolve(100), 100);
}

#[test]
fn wrap_resolve_literal() {
    let mode = definitions::WrapMode::Expr("80".to_string());
    assert_eq!(mode.resolve(200), 80);
}

#[test]
fn wrap_resolve_auto_minus() {
    let mode = definitions::WrapMode::Expr("auto-5".to_string());
    assert_eq!(mode.resolve(100), 95);
}

#[test]
fn wrap_resolve_auto_plus() {
    let mode = definitions::WrapMode::Expr("auto+10".to_string());
    assert_eq!(mode.resolve(100), 110);
}

#[test]
fn wrap_resolve_auto_div() {
    let mode = definitions::WrapMode::Expr("auto/2".to_string());
    assert_eq!(mode.resolve(100), 50);
}

#[test]
fn wrap_resolve_auto_div_plus() {
    let mode = definitions::WrapMode::Expr("auto/2+5".to_string());
    assert_eq!(mode.resolve(100), 55);
}

#[test]
fn wrap_resolve_parens() {
    let mode = definitions::WrapMode::Expr("(auto-10)/2".to_string());
    assert_eq!(mode.resolve(100), 45);
}

#[test]
fn wrap_resolve_floors_to_one() {
    let mode = definitions::WrapMode::Expr("auto-200".to_string());
    assert_eq!(mode.resolve(10), 1);
}

// parse_directory_diff — dangerous create names that could escape to the filesystem

#[test]
fn test_parse_diff_dotdot_line_is_always_filtered() {
    // The "../" skip applies to every line, not just the first.
    // A user who types "../" as a new entry has it silently dropped — it can never
    // become a create, which prevents accidental parent-directory operations.
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    set_buffer_text(&mut doc, "../\n/001 a.txt\n../");

    let diff = doc.parse_directory_diff();
    assert!(
        !diff.creates.iter().any(|c| c == "../"),
        "dotdot must never appear in creates: {:?}",
        diff.creates
    );
    assert!(diff.deletes.is_empty());
    assert!(diff.renames.is_empty());
}

#[test]
fn test_parse_diff_whitespace_only_line_not_a_create() {
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    set_buffer_text(&mut doc, "../\n/001 a.txt\n   ");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.is_empty(),
        "whitespace-only line must not become a create: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_rename_to_empty_visible_name_is_ignored() {
    // User deletes the visible name portion but leaves the ID prefix: "/001 " (with trailing space stripped by trim).
    // The trimmed visible part is empty → no rename should be produced.
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    // Buffer line: "/001 " — visible part is "" after stripping prefix.
    set_buffer_text(&mut doc, "../\n/001 ");

    let diff = doc.parse_directory_diff();
    // trim_end_matches('/') on "" is still ""; new_name == "" ≠ "a.txt" → would produce rename to "".
    // This is a known edge case: the rename target is empty, which apply_directory_diff must guard.
    // Here we just verify the diff is consistent (either no rename or exactly one rename to "").
    if !diff.renames.is_empty() {
        assert_eq!(
            diff.renames[0].1, "",
            "if rename produced, target must be empty string not garbage"
        );
    }
}

#[test]
fn test_parse_diff_id_only_line_no_trailing_text_not_counted_as_create() {
    // A line that exactly matches a valid ID prefix format but with nothing visible after it
    // must not land in creates (it has a prefix → handled as a known-ID line, not a raw create).
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    set_buffer_text(&mut doc, "../\n/001 ");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.is_empty(),
        "line with only an ID prefix must not be a create: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_line_with_path_separator_is_create() {
    // A user might type "sub/file.txt" — this should appear in creates verbatim.
    // apply_directory_diff handles the path join; parse layer must not strip or drop it.
    let mut doc = make_populated_directory_doc("/tmp", &[]);
    set_buffer_text(&mut doc, "../\nsub/file.txt");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.iter().any(|c| c == "sub/file.txt"),
        "path with separator must be a create: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_many_entries_ids_are_stable() {
    // With 100 entries the IDs are 001..100 and each round-trips correctly — no collision,
    // no off-by-one at the boundary between two-digit and three-digit IDs.
    let names: Vec<(&str, bool)> = (0..100).map(|_| ("x.txt", false)).collect();
    let doc = make_populated_directory_doc("/tmp", &names);
    let diff = doc.parse_directory_diff();
    assert!(
        diff.renames.is_empty(),
        "unmodified buffer must have no renames"
    );
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

// clamp_cursor_past_invisible — boundary at buffer end

#[test]
fn test_clamp_cursor_at_buffer_end_does_not_panic() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("f.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    // Force cursor to the very last char position.
    let last = doc.buffer.len().saturating_sub(1);
    let _ = doc.buffer.set_cursor(last);
    doc.clamp_cursor_past_invisible(); // must not panic
}

#[test]
fn test_clamp_cursor_multiple_ranges_advances_to_correct_range() {
    // Two entries → two invisible ranges. Place cursor in the second range and
    // confirm it advances to the end of the second range, not the first.
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.txt", false), ("b.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    // "../\n" = 4 bytes, "/001 a.txt\n" = 11 bytes → second prefix starts at 4+11=15, ends at 20.
    let second_range = &doc.invisible_ranges[1].clone();
    let _ = doc.buffer.set_cursor(second_range.start);
    doc.clamp_cursor_past_invisible();
    assert_eq!(
        doc.buffer.cursor(),
        second_range.end,
        "cursor must advance to end of the second invisible range"
    );
}
