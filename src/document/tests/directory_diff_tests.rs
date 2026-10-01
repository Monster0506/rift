use super::common::*;
use super::*;

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
    set_annotated_buffer(&mut doc, "../\n/001 a.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.deletes.len(), 1);
    assert!(diff.deletes[0].to_string_lossy().contains("b.txt"));
}

#[test]
fn test_parse_diff_new_file_entry() {
    let mut doc = make_populated_directory_doc("/tmp", &[("existing.txt", false)]);
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
    for c in &diff.creates {
        assert_ne!(c, "../", "parent nav must not be treated as a create");
    }
}

#[test]
fn test_parse_diff_empty_lines_ignored() {
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    let _ = doc.buffer.set_cursor(doc.buffer.len());
    let _ = doc.buffer.insert_str("\n");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.is_empty(),
        "empty lines not counted as creates"
    );
}

#[test]
fn test_parse_diff_one_entry_replaced_is_rename() {
    let mut doc = make_populated_directory_doc("/tmp", &[("old.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 new.txt");

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
    let mut doc = make_populated_directory_doc("/tmp", &[("A", true), ("b.c", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 A/\n/002 A/b.c");

    let diff = doc.parse_directory_diff();
    assert!(diff.deletes.is_empty(), "no deletes expected: {:?}", diff);
    assert_eq!(
        diff.renames.len(),
        1,
        "should produce one rename: {:?}",
        diff
    );
    assert!(
        diff.renames[0].0.to_string_lossy().contains("b.c"),
        "rename source should be b.c: {:?}",
        diff.renames[0]
    );
    assert_eq!(diff.renames[0].1, "A/b.c");
    assert!(diff.creates.is_empty(), "no creates expected: {:?}", diff);
}

#[test]
fn test_parse_diff_move_one_of_two_files_into_subdir() {
    let mut doc =
        make_populated_directory_doc("/tmp", &[("A", true), ("b.c", false), ("c.d", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 A/\n/002 b.c\n/003 A/c.d");

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

#[test]
fn test_parse_diff_rename_simple() {
    let mut doc = make_populated_directory_doc("/tmp", &[("test1.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 test1.json");

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
    let mut doc = make_populated_directory_doc(
        "/tmp",
        &[("a.txt", false), ("b.txt", false), ("c.txt", false)],
    );
    set_annotated_buffer(&mut doc, "../\n/001 a.txt\n/002 b.json\n/003 c.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 1);
    assert!(diff.renames[0].0.to_string_lossy().contains("b.txt"));
    assert_eq!(diff.renames[0].1, "b.json");
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_rename_multiple() {
    let mut doc = make_populated_directory_doc("/tmp", &[("foo.txt", false), ("bar.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 foo.rs\n/002 bar.rs");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.renames.len(), 2);
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_rename_with_delete() {
    let mut doc = make_populated_directory_doc(
        "/tmp",
        &[("keep.txt", false), ("old.txt", false), ("gone.txt", false)],
    );
    set_annotated_buffer(&mut doc, "../\n/001 keep.txt\n/002 new.txt");

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
    let mut doc = make_populated_directory_doc("/tmp", &[("original.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 renamed.txt\nbrand_new.txt");

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
    set_annotated_buffer(&mut doc, "../\n/001 a.txt");

    let diff = doc.parse_directory_diff();
    assert_eq!(diff.deletes.len(), 2);
}

#[test]
fn test_parse_diff_rename_to_subdirectory_path() {
    let mut doc = make_populated_directory_doc("/tmp", &[("Playground", true), ("test", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 Playground/\n/002 Playground/test");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.deletes.is_empty(),
        "neither Playground nor test should be deleted: {:?}",
        diff.deletes
    );
    assert!(
        diff.creates.is_empty(),
        "no new entries expected: {:?}",
        diff.creates
    );
    assert_eq!(
        diff.renames.len(),
        1,
        "exactly one rename (test -> Playground/test): {:?}",
        diff
    );
    assert!(
        diff.renames[0].0.to_string_lossy().contains("test"),
        "rename should be from 'test': {:?}",
        diff.renames[0]
    );
    assert_eq!(
        diff.renames[0].1, "Playground/test",
        "rename should be to 'Playground/test': {:?}",
        diff.renames[0]
    );
}

#[test]
fn test_parse_diff_rename_into_dir_while_dir_line_removed_does_not_delete_dir() {
    let mut doc = make_populated_directory_doc("/tmp", &[("Playground", true), ("test", false)]);
    set_annotated_buffer(&mut doc, "../\n/002 Playground/test");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.deletes.is_empty(),
        "Playground/ must be protected from deletion when it is the rename destination parent: {:?}",
        diff.deletes
    );
    assert_eq!(diff.renames.len(), 1, "one rename expected: {:?}", diff);
    assert_eq!(diff.renames[0].1, "Playground/test");
    assert!(
        diff.creates.is_empty(),
        "no creates expected: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_dir_entry_replaced_with_path_is_create_not_rename() {
    let mut doc = make_populated_directory_doc("/tmp", &[("Playground", true)]);
    set_annotated_buffer(&mut doc, "../\n/001 Playground/newfolder/newfile");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.renames.is_empty(),
        "must not rename the dir: {:?}",
        diff.renames
    );
    assert!(
        diff.deletes.is_empty(),
        "Playground/ must not be deleted: {:?}",
        diff.deletes
    );
    assert_eq!(
        diff.creates.len(),
        1,
        "one create expected: {:?}",
        diff.creates
    );
    assert_eq!(diff.creates[0], "Playground/newfolder/newfile");
}

#[test]
fn test_parse_diff_dir_line_replaced_with_path_plus_sibling_create() {
    let mut doc = make_populated_directory_doc("/tmp", &[("Playground", true)]);
    set_annotated_buffer(
        &mut doc,
        "../\n/001 Playground/\nPlayground/newfolder/newfile\nPlayground/newfile",
    );

    let diff = doc.parse_directory_diff();
    assert!(
        diff.renames.is_empty(),
        "no renames expected: {:?}",
        diff.renames
    );
    assert!(
        diff.deletes.is_empty(),
        "no deletes expected: {:?}",
        diff.deletes
    );
    assert_eq!(
        diff.creates.len(),
        2,
        "two creates expected: {:?}",
        diff.creates
    );
    assert!(diff
        .creates
        .iter()
        .any(|c| c == "Playground/newfolder/newfile"));
    assert!(diff.creates.iter().any(|c| c == "Playground/newfile"));
}

#[test]
fn test_parse_diff_file_entry_with_path_prefix_is_still_rename() {
    let mut doc = make_populated_directory_doc("/tmp", &[("Playground", true), ("test", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 Playground/\n/002 Playground/test");

    let diff = doc.parse_directory_diff();
    assert_eq!(
        diff.renames.len(),
        1,
        "one rename (move) expected: {:?}",
        diff.renames
    );
    assert_eq!(diff.renames[0].1, "Playground/test");
    assert!(
        diff.deletes.is_empty(),
        "Playground/ must not be deleted: {:?}",
        diff.deletes
    );
    assert!(
        diff.creates.is_empty(),
        "no creates expected: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_reorder_without_rename_produces_no_diff() {
    let mut doc =
        make_populated_directory_doc("/tmp", &[("alpha.txt", false), ("beta.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/002 beta.txt\n/001 alpha.txt");
    let diff = doc.parse_directory_diff();
    assert!(
        diff.renames.is_empty(),
        "reorder without name change -> no renames: {:?}",
        diff
    );
    assert!(diff.deletes.is_empty());
    assert!(diff.creates.is_empty());
}

#[test]
fn test_parse_diff_entry_with_zero_id_silently_ignored() {
    let mut doc = make_populated_directory_doc("/tmp", &[("real.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 real.txt\n/000 ghost.txt");
    let diff = doc.parse_directory_diff();
    assert!(
        diff.deletes.is_empty(),
        "real.txt should not be deleted: {:?}",
        diff
    );
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
    set_annotated_buffer(&mut doc, "../");
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
fn test_parse_diff_dotdot_line_is_always_filtered() {
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 a.txt\n../");

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
    set_annotated_buffer(&mut doc, "../\n/001 a.txt\n   ");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.is_empty(),
        "whitespace-only line must not become a create: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_rename_to_empty_visible_name_is_ignored() {
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 ");

    let diff = doc.parse_directory_diff();
    if !diff.renames.is_empty() {
        assert_eq!(
            diff.renames[0].1, "",
            "if rename produced, target must be empty string not garbage"
        );
    }
}

#[test]
fn test_parse_diff_id_only_line_no_trailing_text_not_counted_as_create() {
    let mut doc = make_populated_directory_doc("/tmp", &[("a.txt", false)]);
    set_annotated_buffer(&mut doc, "../\n/001 ");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.is_empty(),
        "line with only an ID prefix must not be a create: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_line_with_path_separator_is_create() {
    let mut doc = make_populated_directory_doc("/tmp", &[]);
    set_annotated_buffer(&mut doc, "../\nsub/file.txt");

    let diff = doc.parse_directory_diff();
    assert!(
        diff.creates.iter().any(|c| c == "sub/file.txt"),
        "path with separator must be a create: {:?}",
        diff.creates
    );
}

#[test]
fn test_parse_diff_many_entries_ids_are_stable() {
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
