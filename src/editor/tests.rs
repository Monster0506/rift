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

    let result = editor.remove_document(doc_id);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.severity, ErrorSeverity::Warning);
}

#[test]
fn test_editor_open_file() {
    let mut editor = create_editor();
    editor
        .open_file(Some("new_file.txt".to_string()), false)
        .unwrap();

    assert_eq!(editor.document_manager.tab_count(), 2);
    assert_eq!(editor.document_manager.active_tab_index(), 1);
    assert_eq!(editor.active_document().display_name(), "new_file.txt");

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

    editor.active_document().insert_char('x').unwrap();
    editor
        .open_file(Some("test2.txt".to_string()), false)
        .unwrap();

    assert!(!editor.active_document().is_dirty());
    assert!(editor.document_manager.has_unsaved_changes());

    let clean_id = editor.document_manager.active_document_id().unwrap();

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

    assert_eq!(editor.document_manager.active_tab_index(), 2);

    editor.do_buffer_prev();
    assert_eq!(editor.document_manager.active_tab_index(), 1);
    assert_eq!(editor.active_document().display_name(), "doc1.txt");

    editor.do_buffer_next();
    assert_eq!(editor.document_manager.active_tab_index(), 2);
    assert_eq!(editor.active_document().display_name(), "doc2.txt");

    editor.do_buffer_next();
    assert_eq!(editor.document_manager.active_tab_index(), 0);

    editor.do_buffer_prev();
    assert_eq!(editor.document_manager.active_tab_index(), 2);
}

#[test]
fn test_search_closes_on_success() {
    let mut editor = create_editor();

    editor
        .open_file(Some("test.txt".to_string()), false)
        .unwrap();
    editor
        .active_document()
        .buffer
        .insert_str("hello world")
        .unwrap();

    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::EnterSearchMode,
    ));
    assert_eq!(editor.current_mode, Mode::Search);

    for c in "hello".chars() {
        editor.state.append_to_command_line(c);
    }

    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::Submit,
    ));

    assert_eq!(editor.current_mode, Mode::Normal);
    editor.update_and_render().unwrap();

    let layer = editor
        .render_system
        .compositor
        .get_layer(crate::layer::LayerPriority::FLOATING_WINDOW)
        .unwrap();
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

    editor
        .open_file(Some("test.txt".to_string()), false)
        .unwrap();
    editor
        .active_document()
        .buffer
        .insert_str("hello world")
        .unwrap();

    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::EnterSearchMode,
    ));
    assert_eq!(editor.current_mode, Mode::Search);

    for c in "goodbye".chars() {
        editor.state.append_to_command_line(c);
    }

    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::Submit,
    ));

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

    let tmp = std::env::temp_dir();
    editor.open_explorer(tmp.clone());

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

fn load_text(editor: &mut Editor<MockTerminal>, text: &str) {
    let doc = editor.active_document();
    doc.buffer.move_to_start();
    doc.buffer.insert_str(text).unwrap();
    doc.buffer.move_to_start();
}

#[test]
fn test_g_no_count_goes_to_last_line() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\n");

    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    let last_line = doc.buffer.line_count() - 1;
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(last_line));
}

#[test]
fn test_g_with_count_jumps_to_line() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "alpha\nbeta\ngamma\ndelta\n");

    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(2));
}

#[test]
fn test_g_count_beyond_last_line_clamps() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "one\ntwo\nthree\n");

    editor.pending_count = 999;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    let last_line = doc.buffer.line_count() - 1;
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(last_line));
}

#[test]
fn test_g_count_one_goes_to_first_line() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "first\nsecond\nthird\n");

    editor.pending_count = 0;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));
    editor.pending_count = 1;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(0));
}

#[test]
fn test_goto_line_explicit_n_no_count() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "a\nb\nc\nd\n");

    editor.handle_action(&Action::Editor(EditorAction::GotoLine(2)));

    let doc = editor.active_document();
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(1));
}

#[test]
fn test_f_find_char_forward_moves_to_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('o'),
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 4);
}

#[test]
fn test_f_find_char_forward_not_found_stays() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('z'),
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_f_find_char_backward_moves_to_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");
    editor.active_document().buffer.set_cursor(10).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharBackward('h'),
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_find_char_pending_sets_state() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::FindCharPending {
        forward: true,
        till: false,
    }));

    assert_eq!(editor.pending_find_char_dir, Some((true, false)));
}

#[test]
fn test_find_char_pending_backward_sets_state() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();

    editor.handle_action(&Action::Editor(EditorAction::FindCharPending {
        forward: false,
        till: false,
    }));

    assert_eq!(editor.pending_find_char_dir, Some((false, false)));
}

#[test]
fn test_f_keybinding_is_registered() {
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('f'));
    assert!(
        action.is_some(),
        "'f' should have a keybinding in Normal mode"
    );
}

#[test]
fn test_shift_f_keybinding_is_registered() {
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('F'));
    assert!(
        action.is_some(),
        "'F' should have a keybinding in Normal mode"
    );
}

#[test]
fn test_t_keybinding_is_registered() {
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('t'));
    assert!(
        action.is_some(),
        "'t' should have a keybinding in Normal mode"
    );
}

#[test]
fn test_shift_t_keybinding_is_registered() {
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('T'));
    assert!(
        action.is_some(),
        "'T' should have a keybinding in Normal mode"
    );
}

#[test]
fn test_till_forward_stops_one_before_target() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('o'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 3);
}

#[test]
fn test_till_backward_stops_one_after_target() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");
    editor.active_document().buffer.set_cursor(10).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharBackward('o'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 8);
}

#[test]
fn test_till_records_last_find_and_repeats() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(
        &mut editor,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
    );

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('C'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 15);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 22);
}

#[test]
fn test_till_direction_not_flipped_by_repeat() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(
        &mut editor,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
    );

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('C'),
    )));
    let pos_before_clone = editor.active_document().buffer.cursor();
    assert_eq!(pos_before_clone, 15);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    let pos_before_copy = editor.active_document().buffer.cursor();
    assert_eq!(pos_before_copy, 22);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    let pos_after_clone = editor.active_document().buffer.cursor();
    assert_eq!(pos_after_clone, 17);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 17);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), pos_before_copy);
}

#[test]
fn test_dG_deletes_from_cursor_to_end_of_file() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\n");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    assert_eq!(editor.active_document().buffer.len(), 0);
}

#[test]
fn test_dG_with_count_deletes_to_specific_line() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\nline4\n");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    assert!(doc.buffer.len() > 0);
    let remaining = doc.buffer.get_total_lines();
    assert!(remaining <= 3);
}

#[test]
fn test_G_outside_operator_pending_just_moves_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\n");

    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    let last_line = doc.buffer.line_count() - 1;
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(last_line));
    assert_eq!(doc.buffer.len(), 18);
}

#[test]
fn test_n_repeats_last_forward_find_in_same_direction() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "ababa\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('b'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 1);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 3);
}

#[test]
fn test_n_repeats_last_backward_find_in_same_direction() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "ababa\n");
    editor.active_document().buffer.set_cursor(4).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharBackward('b'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 3);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 1);
}

#[test]
fn test_shift_n_repeats_find_in_opposite_direction() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "ababa\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('b'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 1);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('b'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 3);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 1);
}

#[test]
fn test_fn_direction_not_flipped_by_repeat() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(
        &mut editor,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
    );

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('C'),
    )));
    let pos_clone = editor.active_document().buffer.cursor();
    assert_eq!(pos_clone, 16, "fC: expected C of Clone at col 16");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    let pos_copy = editor.active_document().buffer.cursor();
    assert_eq!(pos_copy, 23, "n: expected C of Copy at col 23");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), pos_clone);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), pos_clone);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), pos_copy);
}

#[test]
fn test_n_falls_back_to_search_when_no_find_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world hello\n");
    editor.state.last_search_query = Some("hello".to_string());
    editor.state.last_find_char = None;

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 12);
}

#[test]
fn test_shift_n_falls_back_to_prev_search_when_no_find_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world hello\n");
    editor.active_document().buffer.set_cursor(12).unwrap();
    editor.state.last_search_query = Some("hello".to_string());
    editor.state.last_find_char = None;

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_search_clears_last_find_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world hello\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('o'),
    )));
    assert!(editor.state.last_find_char.is_some());

    editor.state.command_line = "hello".to_string();
    editor.handle_mode_management(crate::command::Command::ExecuteSearch);

    assert!(editor.state.last_find_char.is_none());
}

#[test]
fn test_n_with_no_previous_find_stays_put() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_n_keybinding_maps_to_repeat_find_forward() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('n'));
    assert_eq!(
        action,
        Some(&Action::Editor(EditorAction::Move(
            Motion::RepeatFindForward
        ))),
        "'n' should map to RepeatFindForward"
    );
}

#[test]
fn test_shift_n_keybinding_maps_to_repeat_find_backward() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('N'));
    assert_eq!(
        action,
        Some(&Action::Editor(EditorAction::Move(
            Motion::RepeatFindBackward
        ))),
        "'N' should map to RepeatFindBackward"
    );
}
