use super::apply_directory_diff_to_fs;
use crate::document::DirectoryDiff;
use crate::split::tree::SplitNode;
use crate::split::window::WindowId;
use crate::test_utils::MockTerminal;
use std::path::Path;

fn rename_diff(dir: &Path, pairs: &[(&str, &str)]) -> DirectoryDiff {
    DirectoryDiff {
        renames: pairs
            .iter()
            .map(|(old, new)| (dir.join(old), (*new).to_string()))
            .collect(),
        ..DirectoryDiff::default()
    }
}

#[test]
fn bug_explorer_rename_overwrites_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), b"from a").unwrap();
    std::fs::write(dir.path().join("b.txt"), b"from b").unwrap();

    let diff = rename_diff(dir.path(), &[("a.txt", "b.txt")]);
    let errors = apply_directory_diff_to_fs(dir.path(), &diff);

    assert_eq!(errors.len(), 1);
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"from a");
    assert_eq!(std::fs::read(dir.path().join("b.txt")).unwrap(), b"from b");
}

#[test]
fn rename_dir_onto_existing_dir_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("a")).unwrap();
    std::fs::create_dir(dir.path().join("b")).unwrap();
    std::fs::write(dir.path().join("a").join("f"), b"from a").unwrap();

    let diff = rename_diff(dir.path(), &[("a", "b")]);
    let errors = apply_directory_diff_to_fs(dir.path(), &diff);

    assert_eq!(errors.len(), 1);
    assert!(dir.path().join("a").join("f").exists());
    assert!(!dir.path().join("b").join("f").exists());
}

#[test]
fn rename_swap_exchanges_both_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), b"from a").unwrap();
    std::fs::write(dir.path().join("b.txt"), b"from b").unwrap();

    let diff = rename_diff(dir.path(), &[("a.txt", "b.txt"), ("b.txt", "a.txt")]);
    let errors = apply_directory_diff_to_fs(dir.path(), &diff);

    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"from b");
    assert_eq!(std::fs::read(dir.path().join("b.txt")).unwrap(), b"from a");
}

#[test]
fn rename_chain_shifts_without_clobbering() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"A").unwrap();
    std::fs::write(dir.path().join("b"), b"B").unwrap();

    let diff = rename_diff(dir.path(), &[("a", "b"), ("b", "c")]);
    let errors = apply_directory_diff_to_fs(dir.path(), &diff);

    assert!(errors.is_empty(), "{errors:?}");
    assert!(!dir.path().join("a").exists());
    assert_eq!(std::fs::read(dir.path().join("b")).unwrap(), b"A");
    assert_eq!(std::fs::read(dir.path().join("c")).unwrap(), b"B");
}

#[test]
fn rename_chain_ending_on_existing_file_is_refused_whole() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"A").unwrap();
    std::fs::write(dir.path().join("b"), b"B").unwrap();
    std::fs::write(dir.path().join("c"), b"C").unwrap();

    let diff = rename_diff(dir.path(), &[("a", "b"), ("b", "c")]);
    let errors = apply_directory_diff_to_fs(dir.path(), &diff);

    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"A");
    assert_eq!(std::fs::read(dir.path().join("b")).unwrap(), b"B");
    assert_eq!(std::fs::read(dir.path().join("c")).unwrap(), b"C");
}

#[test]
fn two_renames_to_one_name_apply_only_the_first() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"A").unwrap();
    std::fs::write(dir.path().join("b"), b"B").unwrap();

    let diff = rename_diff(dir.path(), &[("a", "c"), ("b", "c")]);
    let errors = apply_directory_diff_to_fs(dir.path(), &diff);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(std::fs::read(dir.path().join("c")).unwrap(), b"A");
    assert_eq!(std::fs::read(dir.path().join("b")).unwrap(), b"B");
}

#[test]
fn case_only_rename_is_not_mistaken_for_a_collision() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), b"x").unwrap();

    let diff = rename_diff(dir.path(), &[("file.txt", "FILE.txt")]);
    let errors = apply_directory_diff_to_fs(dir.path(), &diff);

    assert!(errors.is_empty(), "{errors:?}");
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["FILE.txt"]);
}

fn assert_failed_split_rolls_back(open: fn(&mut super::Editor<MockTerminal>)) {
    let mut editor = super::Editor::with_file(MockTerminal::new(24, 80), None).unwrap();
    let original_doc = editor.split_tree.focused_window().document_id;
    let doc_count = editor.document_manager.documents_iter().count();

    // The focused window is no longer a leaf, so split() must refuse.
    editor.split_tree.root = SplitNode::Leaf(WindowId::new(999));
    open(&mut editor);

    assert!(editor.panel_layout.is_none());
    assert_eq!(editor.split_tree.focused_window().document_id, original_doc);
    assert_eq!(editor.document_manager.documents_iter().count(), doc_count);
}

#[test]
fn explorer_open_rolls_back_when_split_fails() {
    assert_failed_split_rolls_back(|e| e.open_explorer(std::env::temp_dir()));
}

#[test]
fn clipboard_open_rolls_back_when_split_fails() {
    assert_failed_split_rolls_back(super::Editor::open_clipboard);
}

#[test]
fn buffer_list_open_rolls_back_when_split_fails() {
    assert_failed_split_rolls_back(super::Editor::open_buffer_list_panel);
}

#[test]
fn undotree_open_rolls_back_when_split_fails() {
    assert_failed_split_rolls_back(super::Editor::open_undotree_split);
}
