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

// ============================================================
// Split window tests
// ============================================================

fn split_current(editor: &mut Editor<MockTerminal>, direction: crate::split::tree::SplitDirection) {
    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction,
        subcommand: crate::command_line::commands::SplitSubcommand::Current,
    });
}

#[test]
fn test_split_creates_second_window() {
    use crate::split::tree::SplitDirection;
    let mut editor = create_editor();
    assert_eq!(editor.split_tree.window_count(), 1);

    split_current(&mut editor, SplitDirection::Horizontal);
    assert_eq!(editor.split_tree.window_count(), 2);
}

#[test]
fn test_split_file_not_found_emits_error() {
    use crate::split::tree::SplitDirection;
    let mut editor = create_editor();

    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction: SplitDirection::Horizontal,
        subcommand: crate::command_line::commands::SplitSubcommand::File(
            "nonexistent_file_xyz.txt".to_string(),
        ),
    });

    assert_eq!(editor.split_tree.window_count(), 1);
    assert!(!editor.state.error_manager.notifications().is_empty());
}

#[test]
fn test_freeze_isolates_sibling_windows() {
    use crate::split::tree::SplitDirection;
    let mut editor = create_editor();

    split_current(&mut editor, SplitDirection::Vertical);
    assert_eq!(editor.split_tree.window_count(), 2);

    let focused_id = editor.split_tree.focused_window_id();
    let sibling_id = editor
        .split_tree
        .all_window_ids()
        .into_iter()
        .find(|&id| id != focused_id)
        .unwrap();

    let doc_id = editor.split_tree.focused_window().document_id;
    assert_eq!(
        editor
            .split_tree
            .get_window(sibling_id)
            .unwrap()
            .document_id,
        doc_id
    );
    assert!(!editor
        .split_tree
        .get_window(sibling_id)
        .unwrap()
        .is_frozen());

    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction: SplitDirection::Vertical,
        subcommand: crate::command_line::commands::SplitSubcommand::Freeze,
    });

    let sibling = editor.split_tree.get_window(sibling_id).unwrap();
    assert!(sibling.is_frozen());
    assert_ne!(sibling.document_id, doc_id);
    assert_eq!(sibling.original_document_id, Some(doc_id));
    assert!(editor
        .document_manager
        .get_document(sibling.document_id)
        .is_some());
    assert_eq!(editor.document_manager.tab_count(), 1);
}

#[test]
fn test_freeze_does_not_affect_focused_window() {
    use crate::split::tree::SplitDirection;
    let mut editor = create_editor();
    split_current(&mut editor, SplitDirection::Vertical);

    let focused_id = editor.split_tree.focused_window_id();
    let doc_id = editor.split_tree.focused_window().document_id;

    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction: SplitDirection::Vertical,
        subcommand: crate::command_line::commands::SplitSubcommand::Freeze,
    });

    let focused = editor.split_tree.get_window(focused_id).unwrap();
    assert!(!focused.is_frozen());
    assert_eq!(focused.document_id, doc_id);
}

#[test]
fn test_nofreeze_reattaches_siblings_and_writes_back() {
    use crate::split::tree::SplitDirection;
    let mut editor = create_editor();

    editor.active_document().buffer.insert_str("hello").unwrap();
    split_current(&mut editor, SplitDirection::Vertical);

    let focused_id = editor.split_tree.focused_window_id();
    let sibling_id = editor
        .split_tree
        .all_window_ids()
        .into_iter()
        .find(|&id| id != focused_id)
        .unwrap();
    let orig_doc_id = editor.split_tree.focused_window().canonical_document_id();

    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction: SplitDirection::Vertical,
        subcommand: crate::command_line::commands::SplitSubcommand::Freeze,
    });

    let private_id = editor
        .split_tree
        .get_window(sibling_id)
        .unwrap()
        .document_id;
    assert!(editor.document_manager.get_document(private_id).is_some());

    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction: SplitDirection::Vertical,
        subcommand: crate::command_line::commands::SplitSubcommand::NoFreeze,
    });

    let sibling = editor.split_tree.get_window(sibling_id).unwrap();
    assert!(!sibling.is_frozen());
    assert_eq!(sibling.document_id, orig_doc_id);
    assert!(editor.document_manager.get_document(private_id).is_none());
    assert_eq!(editor.document_manager.tab_count(), 1);
}

#[test]
fn test_frozen_window_is_independently_editable() {
    use crate::split::tree::SplitDirection;
    let mut editor = create_editor();

    editor
        .active_document()
        .buffer
        .insert_str("shared")
        .unwrap();

    split_current(&mut editor, SplitDirection::Vertical);

    let focused_id = editor.split_tree.focused_window_id();
    let sibling_id = editor
        .split_tree
        .all_window_ids()
        .into_iter()
        .find(|&id| id != focused_id)
        .unwrap();

    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction: SplitDirection::Vertical,
        subcommand: crate::command_line::commands::SplitSubcommand::Freeze,
    });

    let private_id = editor
        .split_tree
        .get_window(sibling_id)
        .unwrap()
        .document_id;
    editor
        .document_manager
        .get_document_mut(private_id)
        .unwrap()
        .buffer
        .insert_str(" extra")
        .unwrap();

    let orig_doc_id = editor.split_tree.focused_window().canonical_document_id();
    let shared_len = editor
        .document_manager
        .get_document(orig_doc_id)
        .unwrap()
        .buffer
        .len();
    let private_len = editor
        .document_manager
        .get_document(private_id)
        .unwrap()
        .buffer
        .len();
    assert!(private_len > shared_len);
}

#[test]
fn test_nofreeze_from_frozen_window_uses_its_buffer_as_truth() {
    use crate::split::tree::SplitDirection;
    let mut editor = create_editor();

    editor
        .active_document()
        .buffer
        .insert_str("original")
        .unwrap();

    split_current(&mut editor, SplitDirection::Vertical);

    let focused_id = editor.split_tree.focused_window_id();
    let sibling_id = editor
        .split_tree
        .all_window_ids()
        .into_iter()
        .find(|&id| id != focused_id)
        .unwrap();
    let orig_doc_id = editor.split_tree.focused_window().canonical_document_id();

    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction: SplitDirection::Vertical,
        subcommand: crate::command_line::commands::SplitSubcommand::Freeze,
    });

    let private_id = editor
        .split_tree
        .get_window(sibling_id)
        .unwrap()
        .document_id;
    editor
        .document_manager
        .get_document_mut(private_id)
        .unwrap()
        .buffer
        .insert_str(" modified")
        .unwrap();
    let private_len = editor
        .document_manager
        .get_document(private_id)
        .unwrap()
        .buffer
        .len();

    editor.split_tree.set_focus(sibling_id);
    let _ = editor.document_manager.switch_to_document(private_id);

    editor.handle_execution_result(ExecutionResult::SplitWindow {
        direction: SplitDirection::Vertical,
        subcommand: crate::command_line::commands::SplitSubcommand::NoFreeze,
    });

    let shared_len = editor
        .document_manager
        .get_document(orig_doc_id)
        .unwrap()
        .buffer
        .len();
    assert_eq!(shared_len, private_len);
    assert_eq!(
        editor
            .split_tree
            .get_window(focused_id)
            .unwrap()
            .document_id,
        orig_doc_id
    );
    assert_eq!(
        editor
            .split_tree
            .get_window(sibling_id)
            .unwrap()
            .document_id,
        orig_doc_id
    );
}
