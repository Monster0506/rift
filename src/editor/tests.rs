use super::*;
use crate::error::ErrorSeverity;
use crate::test_utils::MockTerminal;

fn create_editor() -> Editor<MockTerminal> {
    let term = MockTerminal::new(24, 80);
    Editor::new(term).unwrap()
}

#[test]
fn test_escape_closes_completion_dropdown_only() {
    use crate::command_line::commands::completion::CompletionCandidate;
    use crate::state::CompletionSession;

    let mut editor = create_editor();

    editor.set_mode(Mode::Command);
    editor.state.command_line = ":e foo".to_string();
    editor.state.command_line_cursor = editor.state.command_line.len();

    let mut session = CompletionSession::new(
        editor.state.command_line.clone(),
        vec![CompletionCandidate {
            text: "edit".into(),
            description: "Edit file".into(),
            is_directory: false,
        }],
        0,
    );
    session.dropdown_open = true;
    session.selected = Some(0);
    editor.state.completion_session = Some(session);

    editor.handle_key_actions(crate::key_handler::KeyAction::ExitCommandMode);

    assert_eq!(editor.current_mode, Mode::Command);
    assert_eq!(editor.state.command_line, ":e foo");
    assert!(editor
        .state
        .completion_session
        .as_ref()
        .is_some_and(|s| !s.dropdown_open));
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
fn test_quit_last_buffer_quits_editor() {
    let mut editor = create_editor();
    assert_eq!(editor.document_manager.tab_count(), 1);

    editor.do_quit(false);

    assert!(
        editor.should_quit,
        ":q on last clean buffer should quit the editor"
    );
}

#[test]
fn test_quit_dirty_last_buffer_refuses() {
    let mut editor = create_editor();
    let original_id = editor.document_manager.active_document_id().unwrap();
    editor.active_document().insert_char('x').unwrap();
    assert!(editor.active_document().is_dirty());

    editor.do_quit(false);
    assert!(
        !editor.should_quit,
        ":q should refuse when last buffer is dirty"
    );
    assert_eq!(
        editor.document_manager.active_document_id().unwrap(),
        original_id,
        "dirty buffer should remain active"
    );

    editor.do_quit(true);
    assert!(
        editor.should_quit,
        ":q! should quit even with dirty last buffer"
    );
}

#[test]
fn test_handle_execution_result_quit_only_checks_current_buffer() {
    let mut editor = create_editor();

    // Make the first buffer dirty, then switch to a clean second buffer
    editor.active_document().insert_char('x').unwrap();
    editor
        .open_file(Some("test2.txt".to_string()), false)
        .unwrap();

    // Active buffer (test2.txt) is clean; a different buffer is dirty
    assert!(!editor.active_document().is_dirty());
    assert!(editor.document_manager.has_unsaved_changes());

    let clean_id = editor.document_manager.active_document_id().unwrap();

    // :q should close the clean active buffer without error
    editor.do_quit(false);
    assert!(!editor.should_quit, ":q should not quit the editor");
    assert_ne!(
        editor.document_manager.active_document_id().unwrap(),
        clean_id,
        "clean buffer should have been closed"
    );
}

#[test]
fn test_handle_execution_result_edit() {
    let mut editor = create_editor();
    editor
        .open_file(Some("test.txt".to_string()), false)
        .unwrap();

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
    editor.do_buffer_prev();
    assert_eq!(editor.document_manager.active_tab_index(), 1);
    assert_eq!(editor.active_document().display_name(), "doc1.txt");

    // Next
    editor.do_buffer_next();
    assert_eq!(editor.document_manager.active_tab_index(), 2);
    assert_eq!(editor.active_document().display_name(), "doc2.txt");

    // Wrap around next
    editor.do_buffer_next();
    assert_eq!(editor.document_manager.active_tab_index(), 0);

    // Wrap around previous
    editor.do_buffer_prev();
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
    editor.do_split_window(
        direction,
        crate::command_line::commands::SplitSubcommand::Current,
    );
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

    editor.do_split_window(
        SplitDirection::Horizontal,
        crate::command_line::commands::SplitSubcommand::File(
            "nonexistent_file_xyz.txt".to_string(),
        ),
    );

    assert_eq!(editor.split_tree.window_count(), 1);
    assert!(!editor.state.error_manager.notifications().is_empty());
}

#[test]
fn test_explorer_toggle_hidden_flips_show_hidden() {
    use crate::action::{Action, EditorAction};
    use crate::document::BufferKind;

    let mut editor = create_editor();

    // Manually open explorer to a temp dir so we have a Directory buffer
    let tmp = std::env::temp_dir();
    editor.open_explorer(tmp.clone());

    // Confirm we have a Directory buffer with show_hidden = false
    let layout = editor
        .panel_layout
        .as_ref()
        .expect("panel layout should exist after open_explorer");
    let dir_doc_id = layout.dir_doc_id;
    {
        let doc = editor.document_manager.get_document(dir_doc_id).unwrap();
        match &doc.kind {
            BufferKind::Directory { show_hidden, .. } => assert!(!show_hidden),
            _ => panic!("expected Directory kind"),
        }
    }

    // Toggle hidden on
    editor.handle_action(&Action::Editor(EditorAction::ExplorerToggleHidden));

    {
        let doc = editor.document_manager.get_document(dir_doc_id).unwrap();
        match &doc.kind {
            BufferKind::Directory { show_hidden, .. } => {
                assert!(show_hidden, "show_hidden should be true after first toggle")
            }
            _ => panic!("expected Directory kind"),
        }
    }

    // Toggle hidden off again
    editor.handle_action(&Action::Editor(EditorAction::ExplorerToggleHidden));

    {
        let doc = editor.document_manager.get_document(dir_doc_id).unwrap();
        match &doc.kind {
            BufferKind::Directory { show_hidden, .. } => assert!(
                !show_hidden,
                "show_hidden should be false after second toggle"
            ),
            _ => panic!("expected Directory kind"),
        }
    }
}
