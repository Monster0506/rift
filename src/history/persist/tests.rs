use super::*;
use crate::history::{EditOperation, EditTransaction, Position, UndoTree};

fn insert_tx(desc: &str, text: &str) -> EditTransaction {
    let mut tx = EditTransaction::new(desc);
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: text
            .chars()
            .map(crate::character::Character::from)
            .collect(),
        len: text.len(),
    });
    tx
}

#[test]
fn round_trip_restores_current_and_descriptions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");

    let mut tree = UndoTree::new();
    tree.push(insert_tx("insert a", "a"), None);
    tree.push(insert_tx("insert b", "b"), None);
    let hash = sha256(b"ab");

    save(dir.path(), &path, &tree, hash).unwrap();
    let loaded = load(dir.path(), &path, hash).expect("hash matches, must restore");

    assert_eq!(loaded.current_seq(), tree.current_seq());
    assert!(loaded.is_at_saved());
    assert_eq!(
        loaded.nodes[&loaded.current_seq()].transaction.description,
        "insert b"
    );
    assert_eq!(loaded.nodes[&1].transaction.description, "insert a");
}

#[test]
fn round_trip_preserves_branch_order_and_redo_direction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");

    let mut tree = UndoTree::new();
    tree.push(insert_tx("a", "a"), None); // seq 1
    tree.push(insert_tx("b1", "x"), None); // seq 2, child of 1
    tree.undo(); // back to seq 1
    tree.push(insert_tx("b2", "y"), None); // seq 3, second child of 1
    let hash = sha256(b"whatever"); // content irrelevant to this check

    save(dir.path(), &path, &tree, hash).unwrap();
    let loaded = load(dir.path(), &path, hash).unwrap();

    let node1 = &loaded.nodes[&1];
    assert_eq!(node1.children, vec![2, 3]);
    assert_eq!(node1.last_visited_child, Some(1));
    assert_eq!(loaded.current_seq(), 3);
}

#[test]
fn undo_and_redo_work_on_a_restored_tree() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");

    let mut tree = UndoTree::new();
    tree.push(insert_tx("a", "a"), None);
    tree.push(insert_tx("b", "b"), None);
    let hash = sha256(b"ab");
    save(dir.path(), &path, &tree, hash).unwrap();

    let mut loaded = load(dir.path(), &path, hash).unwrap();
    assert!(loaded.can_undo());
    assert!(!loaded.can_redo());
    let undone = loaded.undo().unwrap().clone();
    assert_eq!(undone.description, "b");
    assert!(loaded.can_redo());
    let redone = loaded.redo().unwrap().clone();
    assert_eq!(redone.description, "b");
}

#[test]
fn load_rejects_content_hash_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");

    let mut tree = UndoTree::new();
    tree.push(insert_tx("a", "a"), None);
    save(dir.path(), &path, &tree, sha256(b"a")).unwrap();

    assert!(load(dir.path(), &path, sha256(b"different")).is_none());
}

#[test]
fn load_returns_none_when_no_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("never-saved.txt");
    assert!(load(dir.path(), &path, sha256(b"")).is_none());
}

#[test]
fn load_returns_none_for_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(undo_file_path(dir.path(), &path), b"not our format").unwrap();
    assert!(load(dir.path(), &path, sha256(b"")).is_none());
}

#[test]
fn load_rejects_mismatched_canonical_path_even_with_matching_hash() {
    let dir = tempfile::tempdir().unwrap();
    let real_path = dir.path().join("real.txt");
    let mut tree = UndoTree::new();
    tree.push(insert_tx("a", "a"), None);
    let hash = sha256(b"a");
    save(dir.path(), &real_path, &tree, hash).unwrap();

    let other_path = dir.path().join("other.txt");
    let bytes = std::fs::read(undo_file_path(dir.path(), &real_path)).unwrap();
    std::fs::write(undo_file_path(dir.path(), &other_path), &bytes).unwrap();

    assert!(load(dir.path(), &other_path, hash).is_none());
}

#[test]
fn saved_file_starts_with_magic_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    let tree = UndoTree::new();
    save(dir.path(), &path, &tree, sha256(b"")).unwrap();

    let bytes = std::fs::read(undo_file_path(dir.path(), &path)).unwrap();
    assert!(has_magic(&bytes));
    assert!(!has_magic(b"not a rift undo file at all"));
    assert!(!has_magic(b"\x89RiftUD")); // magic prefix truncated
}

#[test]
fn parse_for_display_recovers_tree_without_a_content_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");

    let mut tree = UndoTree::new();
    tree.push(insert_tx("insert 'hi'", "hi"), None);
    save(dir.path(), &path, &tree, sha256(b"hi")).unwrap();

    let bytes = std::fs::read(undo_file_path(dir.path(), &path)).unwrap();
    let parsed = parse_for_display(&bytes).expect("well-formed file must parse");

    assert_eq!(parsed.source_path, path);
    assert_eq!(parsed.content_hash, sha256(b"hi"));
    assert_eq!(parsed.tree.current_seq(), tree.current_seq());
    assert_eq!(parsed.tree.nodes[&1].transaction.description, "insert 'hi'");
}

#[test]
fn parse_for_display_rejects_an_unsupported_format_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    let tree = UndoTree::new();
    save(dir.path(), &path, &tree, sha256(b"")).unwrap();

    let mut bytes = std::fs::read(undo_file_path(dir.path(), &path)).unwrap();
    bytes[MAGIC.len()] = 0xff; // corrupt the version field
    bytes[MAGIC.len() + 1] = 0xff;
    std::fs::write(undo_file_path(dir.path(), &path), &bytes).unwrap();

    assert!(parse_for_display(&bytes).is_err());
    assert!(load(dir.path(), &path, sha256(b"")).is_none());
}

#[test]
fn parse_for_display_rejects_non_undo_file_bytes() {
    assert!(parse_for_display(b"just some regular text").is_err());
}

#[test]
fn expand_tilde_leaves_non_tilde_paths_unchanged() {
    assert_eq!(
        expand_tilde("/absolute/path"),
        std::path::PathBuf::from("/absolute/path")
    );
}

#[test]
fn expand_tilde_replaces_prefix_with_actual_home() {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let Ok(home) = std::env::var(var) else {
        return; // no home dir in this environment; nothing to expand against
    };
    assert_eq!(
        expand_tilde("~/undo"),
        std::path::PathBuf::from(&home).join("undo")
    );
    assert_eq!(expand_tilde("~"), std::path::PathBuf::from(&home));
}

#[test]
fn resolve_undo_dir_uses_configured_path_when_set() {
    let configured = std::path::PathBuf::from("/custom/undo/dir");
    assert_eq!(resolve_undo_dir(Some(&configured)), configured);
}

#[test]
fn resolve_undo_dir_defaults_under_config_dir() {
    let resolved = resolve_undo_dir(None);
    assert!(resolved.ends_with("undofiles"));
}
