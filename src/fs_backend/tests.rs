use super::backend;

#[test]
fn read_file_returns_full_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"hello world").unwrap();

    assert_eq!(backend().read_file(&path).unwrap(), b"hello world");
}

#[test]
fn read_file_errs_for_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.txt");

    assert!(backend().read_file(&path).is_err());
}

#[test]
fn read_file_prefix_truncates_to_max_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"0123456789").unwrap();

    assert_eq!(backend().read_file_prefix(&path, 4).unwrap(), b"0123");
}

#[test]
fn read_file_prefix_returns_whole_file_when_shorter_than_max() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"short").unwrap();

    assert_eq!(backend().read_file_prefix(&path, 100).unwrap(), b"short");
}

#[test]
fn write_file_creates_a_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.txt");

    backend().write_file(&path, b"content").unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"content");
}

#[test]
fn write_file_replaces_existing_content_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing.txt");
    std::fs::write(&path, b"old").unwrap();

    backend().write_file(&path, b"new").unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert!(!dir.path().join("existing.txt~").exists());
}

#[test]
fn path_exists_is_true_for_files_and_dirs_false_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    let missing = dir.path().join("missing.txt");

    assert!(backend().path_exists(dir.path()));
    assert!(backend().path_exists(&file));
    assert!(!backend().path_exists(&missing));
}

#[test]
fn is_dir_distinguishes_files_from_directories() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"x").unwrap();

    assert!(backend().is_dir(dir.path()));
    assert!(!backend().is_dir(&file));
}

#[test]
fn parent_dir_missing_is_true_only_when_parent_does_not_exist() {
    let dir = tempfile::tempdir().unwrap();
    let existing_parent_child = dir.path().join("child.txt");
    let missing_parent_child = dir.path().join("nope").join("child.txt");

    assert!(!backend().parent_dir_missing(&existing_parent_child));
    assert!(backend().parent_dir_missing(&missing_parent_child));
}

#[test]
fn canonicalize_resolves_an_existing_path() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"x").unwrap();

    let canon = backend().canonicalize(&file);
    assert!(canon.is_absolute());
    assert!(canon.ends_with("f.txt"));
}

#[test]
fn canonicalize_still_normalizes_a_nonexistent_path() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope").join("f.txt");

    let canon = backend().canonicalize(&missing);
    assert!(canon.is_absolute());
    assert!(canon.ends_with("nope/f.txt") || canon.ends_with("nope\\f.txt"));
}

#[test]
fn list_children_reports_names_and_dir_flags() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), b"x").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();

    let mut entries = backend().list_children(dir.path()).unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a.txt");
    assert!(!entries[0].is_dir);
    assert_eq!(entries[1].name, "sub");
    assert!(entries[1].is_dir);
}

#[test]
fn create_dir_makes_missing_parents_too() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a").join("b").join("c");

    backend().create_dir(&nested).unwrap();

    assert!(nested.is_dir());
}

#[test]
fn create_file_makes_an_empty_file_and_its_parents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b.txt");

    backend().create_file(&path).unwrap();

    assert!(path.is_file());
    assert_eq!(std::fs::read(&path).unwrap(), Vec::<u8>::new());
}

#[test]
fn rename_moves_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.txt");
    let new = dir.path().join("new.txt");
    std::fs::write(&old, b"content").unwrap();

    backend().rename(&old, &new).unwrap();

    assert!(!old.exists());
    assert_eq!(std::fs::read(&new).unwrap(), b"content");
}

#[test]
fn rename_creates_missing_destination_parents() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.txt");
    let new = dir.path().join("nested").join("new.txt");
    std::fs::write(&old, b"content").unwrap();

    backend().rename(&old, &new).unwrap();

    assert_eq!(std::fs::read(&new).unwrap(), b"content");
}

#[test]
fn delete_recursive_removes_a_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"x").unwrap();

    backend().delete_recursive(&file).unwrap();

    assert!(!file.exists());
}

#[test]
fn delete_recursive_removes_a_directory_and_its_contents() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("f.txt"), b"x").unwrap();

    backend().delete_recursive(&sub).unwrap();

    assert!(!sub.exists());
}
