use super::common::*;

#[test]
fn bug_wq_in_split_quits_editor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    editor.execute_command_line("split");
    assert_eq!(editor.split_tree.window_count(), 2);

    type_text(&mut editor, "hello");
    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(
        !editor.should_quit,
        ":wq in a split must only close the split"
    );
    assert_eq!(editor.split_tree.window_count(), 1);
    assert!(std::fs::read_to_string(&path).unwrap().contains("hello"));
}

#[test]
fn bug_wq_quits_discarding_other_dirty_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "").unwrap();
    std::fs::write(&b, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &a);
    let a_id = editor.active_document_id();
    type_text(&mut editor, "unsaved");
    open_and_load(&mut editor, &b);

    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(!editor.should_quit, ":wq must not exit with a.txt dirty");
    let a_doc = editor
        .document_manager
        .get_document(a_id)
        .expect("a.txt must stay open");
    assert!(a_doc.is_dirty());
}

#[test]
fn bug_explorer_wq_quits_editor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    editor.open_explorer(dir.path().to_path_buf());
    drain_jobs(&mut editor);
    assert!(editor.panel_layout.is_some());

    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(
        !editor.should_quit,
        ":wq in the explorer must only close it"
    );
    assert!(editor.panel_layout.is_none());
}
