use super::*;
use crate::test_utils::MockTerminal;

#[test]
fn with_file_targeting_a_directory_opens_the_explorer_there() {
    let dir = tempfile::tempdir().unwrap();
    let dir_path = dir.path().to_path_buf();
    let dir_path_str = dir_path.to_string_lossy().into_owned();

    let editor = Editor::with_file(MockTerminal::new(24, 80), Some(&dir_path_str))
        .expect("launching against a directory must not error");

    let layout = editor
        .panel_layout
        .as_ref()
        .expect("explorer panel should be open");
    assert_eq!(layout.kind, PanelKind::FileExplorer);

    let dir_doc = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .expect("explorer dir document should exist");
    assert_eq!(dir_doc.directory_path(), Some(&dir_path));
}

#[test]
fn with_file_targeting_a_file_does_not_open_the_explorer() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("plain.txt");
    std::fs::write(&file_path, "hello").unwrap();
    let file_path_str = file_path.to_string_lossy().into_owned();

    let editor = Editor::with_file(MockTerminal::new(24, 80), Some(&file_path_str)).unwrap();

    assert!(editor.panel_layout.is_none());
    let doc = editor.document_manager.active_document().unwrap();
    assert_eq!(
        doc.path().and_then(|p| p.file_name()),
        Some(file_path.file_name().unwrap())
    );
}
