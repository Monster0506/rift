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
    editor.active_document().insert_char('x').unwrap();
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
    editor.active_document().insert_char('x').unwrap();

    // Should not quit if unsaved and no bang
    editor.handle_execution_result(ExecutionResult::Quit { bangs: 0 });
    assert!(!editor.should_quit);

    // Should quit with bang
    editor.handle_execution_result(ExecutionResult::Quit { bangs: 1 });
    assert!(editor.should_quit);
}

#[test]
fn test_handle_execution_result_quit_unsaved_other_buffer() {
    let mut editor = create_editor();

    // Open a second buffer and make the first one dirty
    editor.active_document().insert_char('x').unwrap();
    editor
        .open_file(Some("test2.txt".to_string()), false)
        .unwrap();

    // Now the active document is test2.txt (not dirty), but first buffer is dirty
    assert!(!editor.active_document().is_dirty());
    assert!(editor.document_manager.has_unsaved_changes());

    // Should not quit because another buffer has unsaved changes
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

#[test]
fn test_search_closes_on_success() {
    let mut editor = create_editor();

    // Open a file with content
    editor
        .open_file(Some("test.txt".to_string()), false)
        .unwrap();
    editor
        .active_document()
        .buffer
        .insert_str("hello world")
        .unwrap();

    // Enter Search Mode
    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::EnterSearchMode,
    ));
    assert_eq!(editor.current_mode, Mode::Search);

    // Type "hello"
    for c in "hello".chars() {
        editor.state.append_to_command_line(c);
    }

    // Submit Search
    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::Submit,
    ));

    // Verify: Mode Normal, Layer Cleared
    assert_eq!(editor.current_mode, Mode::Normal);
    editor.update_and_render().unwrap();

    let layer = editor
        .render_system
        .compositor
        .get_layer(crate::layer::LayerPriority::FLOATING_WINDOW)
        .unwrap();
    // Check if layer is empty
    for row in 0..layer.rows() {
        for col in 0..layer.cols() {
            assert!(
                layer.get_cell(row, col).is_none(),
                "Layer should be empty on success"
            );
        }
    }
}

#[test]
fn test_search_stays_open_on_failure() {
    let mut editor = create_editor();

    // Open a file with content
    editor
        .open_file(Some("test.txt".to_string()), false)
        .unwrap();
    editor
        .active_document()
        .buffer
        .insert_str("hello world")
        .unwrap();

    // Enter Search Mode
    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::EnterSearchMode,
    ));
    assert_eq!(editor.current_mode, Mode::Search);

    // Type "goodbye" (not in text)
    for c in "goodbye".chars() {
        editor.state.append_to_command_line(c);
    }

    // Submit Search
    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::Submit,
    ));

    // Verify: Mode Search, Layer NOT Cleared
    assert_eq!(
        editor.current_mode,
        Mode::Search,
        "Should stay in Search mode on failure"
    );
    editor.update_and_render().unwrap();

    let layer = editor
        .render_system
        .compositor
        .get_layer(crate::layer::LayerPriority::FLOATING_WINDOW);
    assert!(layer.is_some(), "Layer should exist");
    // Check if layer has content (search bar)
    let layer = layer.unwrap();
    let mut has_content = false;
    for row in 0..layer.rows() {
        for col in 0..layer.cols() {
            if layer.get_cell(row, col).is_some() {
                has_content = true;
                break;
            }
        }
        if has_content {
            break;
        }
    }
    assert!(has_content, "Search bar should remain visible on failure");
}
