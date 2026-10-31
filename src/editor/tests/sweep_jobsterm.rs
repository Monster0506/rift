use super::common::*;

#[test]
fn wq_then_typing_before_save_completes_quits_with_unsaved_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    type_text(&mut editor, "saved");

    editor.execute_command_line("wq");
    type_text(&mut editor, "!");
    drain_jobs(&mut editor);

    assert!(
        !editor.should_quit,
        "an edit made after :wq started must cancel the quit"
    );
    assert!(editor.active_document().is_dirty());
}
