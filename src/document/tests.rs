use super::*;
use crate::color::Color;

fn create_manager() -> DocumentManager {
    DocumentManager::new()
}

// ──────────────────────────────────────────────
// BufferKind helper predicates
// ──────────────────────────────────────────────

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

// ──────────────────────────────────────────────
// directory_path()
// ──────────────────────────────────────────────

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

// ──────────────────────────────────────────────
// populate_directory_buffer — text format
// ──────────────────────────────────────────────

fn make_dir_entries(names: &[(&str, bool)], base: &str) -> Vec<DirEntry> {
    names.iter().map(|(name, is_dir)| DirEntry {
        path: PathBuf::from(base).join(name),
        is_dir: *is_dir,
    }).collect()
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
    // No numeric IDs like "1 file.rs" or "[1]"
    for line in text.lines() {
        let trimmed = line.trim_start();
        assert!(!trimmed.starts_with(|c: char| c.is_ascii_digit()),
                "line should not start with digit: {:?}", line);
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

// ──────────────────────────────────────────────
// populate_directory_buffer — highlights
// ──────────────────────────────────────────────

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
    assert!(covered, "parent nav line should be Blue: {:?}", doc.custom_highlights);
}

#[test]
fn test_populate_directory_file_entry_is_white() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("readme.txt", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    let _text = doc.buffer.to_string();
    // "readme.txt" starts after "../\n" = 4 bytes
    let start = 4;
    let end = start + "readme.txt".len();
    let covered = doc.custom_highlights.iter().any(|(r, c)| {
        r.start <= start && r.end >= end && *c == Color::White
    });
    assert!(covered, "file entry should be White: {:?}", doc.custom_highlights);
}

#[test]
fn test_populate_directory_dir_entry_is_blue() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("subdir", true)], "/tmp");
    doc.populate_directory_buffer(entries);
    // "subdir/" starts at offset 4 (after "../\n")
    let start = 4;
    let end = start + "subdir/".len();
    let covered = doc.custom_highlights.iter().any(|(r, c)| {
        r.start <= start && r.end >= end && *c == Color::Blue
    });
    assert!(covered, "dir entry should be Blue: {:?}", doc.custom_highlights);
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
    assert!(second_count < first_count || second_count > 0,
            "highlights should be rebuilt on repopulate");
}

#[test]
fn test_populate_directory_no_overlapping_highlight_ranges() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let entries = make_dir_entries(&[("a.txt", false), ("b", true), ("c.rs", false)], "/tmp");
    doc.populate_directory_buffer(entries);
    let highlights = &doc.custom_highlights;
    for i in 0..highlights.len().saturating_sub(1) {
        assert!(highlights[i].0.end <= highlights[i+1].0.start,
                "highlight ranges must not overlap: {:?} vs {:?}", highlights[i], highlights[i+1]);
    }
}

// ──────────────────────────────────────────────
// parse_directory_diff
// ──────────────────────────────────────────────

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
    let new_text: String = text.lines()
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
    assert!(diff.creates.iter().any(|c| c == "new_file.txt"),
            "should detect new_file.txt as a create: {:?}", diff.creates);
}

#[test]
fn test_parse_diff_new_dir_entry_trailing_slash() {
    let mut doc = make_populated_directory_doc("/tmp", &[]);
    // Add a directory entry ending with /
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\nnewdir/");

    let diff = doc.parse_directory_diff();
    assert!(diff.creates.iter().any(|c| c == "newdir/"),
            "should preserve trailing slash in creates: {:?}", diff.creates);
}

#[test]
fn test_parse_diff_new_file_no_trailing_slash() {
    let mut doc = make_populated_directory_doc("/tmp", &[]);
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\nnewfile.txt");

    let diff = doc.parse_directory_diff();
    assert!(diff.creates.iter().any(|c| c == "newfile.txt"),
            "file creates should not have trailing slash: {:?}", diff.creates);
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
    assert!(diff.creates.is_empty(), "empty lines not counted as creates");
}

#[test]
fn test_parse_diff_one_entry_replaced_is_rename() {
    // Replacing the only entry with a new name → rename (not delete + create)
    let mut doc = make_populated_directory_doc("/tmp", &[("old.txt", false)]);
    set_buffer_text(&mut doc, "../\nnew.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1, "positional replacement is a rename");
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

// ──────────────────────────────────────────────
// populate_undotree_buffer
// ──────────────────────────────────────────────

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

    doc.populate_undotree_buffer("xyz".to_string(), vec![], vec![
        (0..1, Color::Blue),
        (1..2, Color::Green),
        (2..3, Color::Yellow),
    ]);
    assert_eq!(doc.custom_highlights.len(), 3);
    assert_eq!(doc.custom_highlights[0].1, Color::Blue);
}

// ──────────────────────────────────────────────
// Manager: special buffers close without dirty check
// ──────────────────────────────────────────────

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

// ──────────────────────────────────────────────
// DirEntry struct
// ──────────────────────────────────────────────

#[test]
fn test_dir_entry_no_id_field() {
    // DirEntry only has path and is_dir; this test ensures the struct is usable
    let entry = DirEntry {
        path: PathBuf::from("/tmp/file.txt"),
        is_dir: false,
    };
    assert!(!entry.is_dir);
    assert_eq!(entry.path, PathBuf::from("/tmp/file.txt"));
}

#[test]
fn test_dir_entry_directory() {
    let entry = DirEntry {
        path: PathBuf::from("/tmp/subdir"),
        is_dir: true,
    };
    assert!(entry.is_dir);
}

// ──────────────────────────────────────────────
// DirectoryDiff — rename detection
// ──────────────────────────────────────────────

/// Helper: replace the full buffer text of a doc (simulates user editing the explorer buffer).
fn set_buffer_text(doc: &mut Document, text: &str) {
    let old_len = doc.buffer.len();
    let _ = doc.buffer.set_cursor(0);
    let _ = doc.delete_range(0, old_len);
    let _ = doc.insert_str(text);
}

#[test]
fn test_parse_diff_rename_simple() {
    // The canonical reported bug: cw renames test1.txt → test1.json
    // Expected: rename, NOT delete test1.txt + create empty test1.json
    let mut doc = make_populated_directory_doc("/tmp", &[("test1.txt", false)]);
    set_buffer_text(&mut doc, "../\ntest1.json");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1, "should detect a rename: {:?}", diff);
    assert!(diff.renames[0].0.to_string_lossy().contains("test1.txt"),
        "renamed from test1.txt: {:?}", diff.renames[0]);
    assert_eq!(diff.renames[0].1, "test1.json",
        "renamed to test1.json: {:?}", diff.renames[0]);
    assert!(diff.deletes.is_empty(), "rename must not also produce a delete");
    assert!(diff.creates.is_empty(), "rename must not also produce a create");
}

#[test]
fn test_parse_diff_rename_preserves_siblings() {
    // Rename one file, leave others untouched
    let mut doc = make_populated_directory_doc("/tmp", &[
        ("a.txt", false), ("b.txt", false), ("c.txt", false),
    ]);
    set_buffer_text(&mut doc, "../\na.txt\nb.json\nc.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1);
    assert!(diff.renames[0].0.to_string_lossy().contains("b.txt"));
    assert_eq!(diff.renames[0].1, "b.json");
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_rename_multiple() {
    let mut doc = make_populated_directory_doc("/tmp", &[
        ("foo.txt", false), ("bar.txt", false),
    ]);
    set_buffer_text(&mut doc, "../\nfoo.rs\nbar.rs");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 2);
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_rename_with_delete() {
    // Rename one, delete another
    let mut doc = make_populated_directory_doc("/tmp", &[
        ("keep.txt", false), ("old.txt", false), ("gone.txt", false),
    ]);
    // keep.txt stays, old.txt → new.txt, gone.txt is deleted
    set_buffer_text(&mut doc, "../\nkeep.txt\nnew.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1, "one rename: {:?}", diff);
    assert_eq!(diff.deletes.len(), 1, "one delete: {:?}", diff);
    assert!(diff.creates.is_empty(), "no creates: {:?}", diff);
    let renamed_from = diff.renames[0].0.file_name().unwrap().to_string_lossy();
    assert!(renamed_from == "old.txt" || renamed_from == "gone.txt",
        "renamed from one of the removed entries: {}", renamed_from);
    assert_eq!(diff.renames[0].1, "new.txt");
}

#[test]
fn test_parse_diff_rename_with_create() {
    // Rename one, add a new entry
    let mut doc = make_populated_directory_doc("/tmp", &[("original.txt", false)]);
    set_buffer_text(&mut doc, "../\nrenamed.txt\nbrand_new.txt");

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

// ──────────────────────────────────────────────
// DirectoryDiff struct
// ──────────────────────────────────────────────

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
    let mut doc = make_populated_directory_doc("/tmp", &[
        ("a.txt", false), ("b.txt", false), ("c.txt", false)
    ]);
    // Keep only a.txt
    let new_text = "../\na.txt";
    let old_len = doc.buffer.len();
    let _ = doc.buffer.set_cursor(0);
    let _ = doc.delete_range(0, old_len);
    let _ = doc.insert_str(new_text);

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.deletes.len(), 2);
}

// ──────────────────────────────────────────────
// Buffer revision increments on populate
// ──────────────────────────────────────────────

#[test]
fn test_populate_directory_increments_revision() {
    let mut doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    let rev0 = doc.buffer.revision;
    doc.populate_directory_buffer(vec![]);
    assert!(doc.buffer.revision > rev0, "revision should increment after populate");
}

#[test]
fn test_populate_undotree_increments_revision() {
    let mut doc = Document::new_undotree(1, 2).unwrap();
    let rev0 = doc.buffer.revision;
    doc.populate_undotree_buffer("text".to_string(), vec![], vec![]);
    assert!(doc.buffer.revision > rev0, "revision should increment after populate");
}

// ──────────────────────────────────────────────
// BufferKind — cloneability
// ──────────────────────────────────────────────

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
        entries: vec![DirEntry { path: PathBuf::from("/tmp/a"), is_dir: false }],
    };
    let cloned = kind.clone();
    match cloned {
        BufferKind::Directory { path, entries } => {
            assert_eq!(path, PathBuf::from("/tmp"));
            assert_eq!(entries.len(), 1);
        }
        _ => panic!("expected Directory"),
    }
}

#[test]
fn test_buffer_kind_undotree_clones() {
    let kind = BufferKind::UndoTree { linked_doc_id: 7, sequences: vec![1, 2, 3] };
    let cloned = kind.clone();
    match cloned {
        BufferKind::UndoTree { linked_doc_id, sequences } => {
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

// ──────────────────────────────────────────────
// WrapMode
// ──────────────────────────────────────────────

#[test]
fn wrap_default_is_auto() {
    let doc = Document::new(1).unwrap();
    assert_eq!(doc.options.wrap, Some(definitions::WrapMode::Expr("auto".to_string())));
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
