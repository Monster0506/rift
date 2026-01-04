use super::*;
use crate::test_utils::MockTerminal;

fn create_editor() -> Editor<MockTerminal> {
    let term = MockTerminal::new(24, 80);
    Editor::new(term).unwrap()
}

#[test]
fn test_editor_initial_state() {
    let editor = create_editor();
    assert_eq!(editor.document_manager.tab_count(), 1);
    assert_eq!(editor.document_manager.active_tab_index(), 0);
}

#[test]
fn test_editor_remove_last_tab() {
    let mut editor = create_editor();
    let doc_id = editor.document_manager.get_document_id_at(0).unwrap();

    // Removing the only tab should create a new empty one
    let result = editor.remove_document(doc_id);
    assert!(result.is_ok());
    assert_eq!(editor.document_manager.tab_count(), 1);
    assert_ne!(
        editor.document_manager.get_document_id_at(0).unwrap(),
        doc_id,
        "Should have a new doc ID"
    );
}

#[test]
fn test_editor_remove_dirty_tab() {
    let mut editor = create_editor();
    editor.active_document().mark_dirty();
    let doc_id = editor.document_manager.get_document_id_at(0).unwrap();

    // Removing a dirty tab should return a warning
    let result = editor.remove_document(doc_id);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.severity, ErrorSeverity::Warning);
}

#[test]
fn test_editor_open_file() {
    let mut editor = create_editor();
    // Open a new "file" (doesn't exist on disk, should create empty buffer)
    editor
        .open_file(Some("new_file.txt".to_string()), false)
        .unwrap();

    assert_eq!(editor.document_manager.tab_count(), 2);
    assert_eq!(editor.document_manager.active_tab_index(), 1);
    assert_eq!(editor.active_document().display_name(), "new_file.txt");

    // Open same file again, should just switch
    editor
        .open_file(Some("new_file.txt".to_string()), false)
        .unwrap();
    assert_eq!(editor.document_manager.tab_count(), 2);
    assert_eq!(editor.document_manager.active_tab_index(), 1);
}

#[test]
fn test_handle_execution_result_quit() {
    let mut editor = create_editor();
    editor.handle_execution_result(ExecutionResult::Quit { bangs: 0 });
    assert!(editor.should_quit);
}

#[test]
fn test_handle_execution_result_quit_unsaved() {
    let mut editor = create_editor();
    editor.active_document().mark_dirty();

    // Should not quit if unsaved and no bang
    editor.handle_execution_result(ExecutionResult::Quit { bangs: 0 });
    assert!(!editor.should_quit);

    // Should quit with bang
    editor.handle_execution_result(ExecutionResult::Quit { bangs: 1 });
    assert!(editor.should_quit);
}

#[test]
fn test_handle_execution_result_edit() {
    let mut editor = create_editor();
    editor.handle_execution_result(ExecutionResult::Edit {
        path: Some("test.txt".to_string()),
        bangs: 0,
    });

    assert_eq!(editor.document_manager.tab_count(), 2);
    assert_eq!(editor.active_document().display_name(), "test.txt");
}

#[test]
fn test_handle_execution_result_buffer_navigation() {
    let mut editor = create_editor();
    editor
        .open_file(Some("doc1.txt".to_string()), false)
        .unwrap();
    editor
        .open_file(Some("doc2.txt".to_string()), false)
        .unwrap();

    // We have [unnamed, doc1, doc2]
    assert_eq!(editor.document_manager.active_tab_index(), 2);

    // Previous
    editor.handle_execution_result(ExecutionResult::BufferPrevious { bangs: 0 });
    assert_eq!(editor.document_manager.active_tab_index(), 1);
    assert_eq!(editor.active_document().display_name(), "doc1.txt");

    // Next
    editor.handle_execution_result(ExecutionResult::BufferNext { bangs: 0 });
    assert_eq!(editor.document_manager.active_tab_index(), 2);
    assert_eq!(editor.active_document().display_name(), "doc2.txt");

    // Wrap around next
    editor.handle_execution_result(ExecutionResult::BufferNext { bangs: 0 });
    assert_eq!(editor.document_manager.active_tab_index(), 0);

    // Wrap around previous
    editor.handle_execution_result(ExecutionResult::BufferPrevious { bangs: 0 });
    assert_eq!(editor.document_manager.active_tab_index(), 2);
}
