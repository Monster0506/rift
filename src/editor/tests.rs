use super::*;
use crate::error::ErrorSeverity;
use crate::test_utils::MockTerminal;

fn create_editor() -> Editor<MockTerminal> {
    let term = MockTerminal::new(24, 80);
    Editor::new(term).unwrap()
}

fn create_editor_sized(rows: u16, cols: u16) -> Editor<MockTerminal> {
    Editor::new(MockTerminal::new(rows, cols)).unwrap()
}

fn render_ascii(editor: &mut Editor<MockTerminal>) -> String {
    editor.update_and_render().unwrap();
    let rows = editor.render_system.compositor.rows();
    let cols = editor.render_system.compositor.cols();
    let cells = editor.render_system.compositor.get_composited_slice();
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| cells[r * cols + c].to_char())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn ring_text(editor: &Editor<MockTerminal>, index: usize) -> Option<String> {
    editor.clipboard_ring.get(index).map(|chars| {
        chars
            .iter()
            .map(crate::character::Character::to_char_lossy)
            .collect()
    })
}

fn do_vsplit(editor: &mut Editor<MockTerminal>) {
    editor.do_split_window(
        crate::split::tree::SplitDirection::Vertical,
        crate::command_line::commands::SplitSubcommand::Current,
    );
    editor.update_and_render().unwrap();
}

fn do_resize_pane(editor: &mut Editor<MockTerminal>, delta: i32) {
    editor.do_split_window(
        crate::split::tree::SplitDirection::Vertical,
        crate::command_line::commands::SplitSubcommand::Resize(delta),
    );
    editor.update_and_render().unwrap();
}

fn set_content(editor: &mut Editor<MockTerminal>, text: &str) {
    let doc = editor.active_document();
    doc.buffer.move_to_start();
    let len = doc.buffer.len();
    for _ in 0..len {
        doc.buffer.delete_forward();
    }
    doc.buffer.insert_str(text).unwrap();
    doc.buffer.move_to_start();
}

fn divider_cols(screen: &str) -> Vec<usize> {
    screen
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .enumerate()
        .filter(|(_, c)| *c == '│')
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn test_vsplit_divider_appears() {
    let mut editor = create_editor_sized(10, 40);
    set_content(&mut editor, "hello world\n");
    do_vsplit(&mut editor);
    let screen = render_ascii(&mut editor);
    assert!(
        screen.contains('│'),
        "vsplit divider should be visible\n{}",
        screen
    );
}

#[test]
fn test_resize_pane_moves_divider() {
    let mut editor = create_editor_sized(10, 40);
    set_content(&mut editor, "hello world\n");
    do_vsplit(&mut editor);

    let before = render_ascii(&mut editor);
    let before_col = before.lines().next().and_then(|l| l.find('│'));

    do_resize_pane(&mut editor, -5);

    let after = render_ascii(&mut editor);
    let after_col = after.lines().next().and_then(|l| l.find('│'));

    assert!(
        before_col.is_some() && after_col.is_some(),
        "divider should be present before and after\nbefore:\n{}\nafter:\n{}",
        before,
        after
    );
    assert_ne!(
        before_col, after_col,
        "divider column should shift\nbefore:\n{}\nafter:\n{}",
        before, after
    );
}

#[test]
fn test_resize_pane_only_shifts_divider_not_all_content() {
    let mut editor = create_editor_sized(10, 60);
    set_content(&mut editor, "hello world\n");
    do_vsplit(&mut editor); // [A | B]
    do_vsplit(&mut editor); // [A | B | C]

    let before = render_ascii(&mut editor);
    let before_divs = divider_cols(&before);

    do_resize_pane(&mut editor, -5);

    let after = render_ascii(&mut editor);
    let after_divs = divider_cols(&after);

    assert_eq!(
        before_divs.len(),
        after_divs.len(),
        "divider count unchanged\nbefore:\n{}\nafter:\n{}",
        before,
        after
    );
    assert_eq!(
        before_divs[0], after_divs[0],
        "left divider fixed\nbefore:{:?} after:{:?}",
        before_divs, after_divs
    );
    assert_ne!(
        before_divs[1], after_divs[1],
        "right divider moved\nbefore:{:?} after:{:?}",
        before_divs, after_divs
    );
}

#[test]
fn test_pane_content_stays_within_boundary_after_resize() {
    let mut editor = create_editor_sized(10, 40);
    set_content(&mut editor, "AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHH12345678\n");
    do_vsplit(&mut editor);
    do_resize_pane(&mut editor, -5);

    let screen = render_ascii(&mut editor);
    let cols = editor.render_system.compositor.cols();

    for line in screen.lines() {
        assert_eq!(
            line.chars().count(),
            cols,
            "row must be exactly {} chars wide",
            cols
        );
    }
    let div_col = screen.lines().next().and_then(|l| l.find('│'));
    assert!(
        div_col.map(|c| c > 0 && c < cols - 1).unwrap_or(false),
        "divider should be inside the screen\n{}",
        screen
    );
}

#[test]
fn test_terminal_resize_updates_layout() {
    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");
    do_vsplit(&mut editor);

    editor.term.size = (24, 60);
    editor.render_system.resize(24, 60);
    editor.update_and_render().unwrap();

    assert_eq!(
        editor.render_system.compositor.cols(),
        60,
        "compositor should reflect new terminal width"
    );
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

    assert!(matches!(
        editor.pending_grammar,
        Some(super::pending_grammar::PendingGrammar::FindChar {
            forward: true,
            till: false
        })
    ));
}

#[test]
fn test_find_char_pending_backward_sets_state() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();

    editor.handle_action(&Action::Editor(EditorAction::FindCharPending {
        forward: false,
        till: false,
    }));

    assert!(matches!(
        editor.pending_grammar,
        Some(super::pending_grammar::PendingGrammar::FindChar {
            forward: false,
            till: false
        })
    ));
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
fn test_dt_on_char_not_found_does_nothing() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('a'),
    )));

    assert_eq!(editor.active_document().buffer.len(), 6);
    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_dt_does_not_cross_line_boundary() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "bcdef\naXXa\n");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('a'),
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "bcdef\naXXa\n");
    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_dtf_on_abcdef_leaves_f_only() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('f'),
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "f");
}

#[test]
fn test_dta_with_second_a_leaves_only_that_a() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abca");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('a'),
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "a");
}

#[test]
fn test_tf_move_still_stops_one_before_target() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('f'),
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 4);
}

#[test]
fn test_dg_deletes_from_cursor_to_end_of_file() {
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
fn test_dg_with_count_deletes_to_specific_line() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\nline4\n");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    assert!(!doc.buffer.is_empty());
    let remaining = doc.buffer.get_total_lines();
    assert!(remaining <= 3);
}

#[test]
fn test_g_outside_operator_pending_just_moves_cursor() {
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

#[test]
fn test_display_map_cache_populated_after_command() {
    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");

    // Cache must be absent before any command.
    assert!(
        editor.display_map_cache.is_none(),
        "cache should start empty"
    );

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Right,
        1,
    ));

    // Cache must be populated with the buffer's current revision.
    let rev = editor.active_document().buffer.revision;
    match &editor.display_map_cache {
        Some((_, cached_rev, _, _)) => assert_eq!(*cached_rev, rev),
        None => panic!("display_map_cache should be populated after a command"),
    }
}

#[test]
fn test_display_map_cache_revision_stable_across_moves() {
    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Right,
        1,
    ));
    let rev_after_first = editor.display_map_cache.as_ref().map(|(_, rev, _, _)| *rev);

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Right,
        1,
    ));
    let rev_after_second = editor.display_map_cache.as_ref().map(|(_, rev, _, _)| *rev);

    // Buffer revision unchanged (no mutations), so cached revision should be the same.
    assert_eq!(
        rev_after_first, rev_after_second,
        "cache revision should not change between non-mutating commands"
    );
}

#[test]
fn test_display_map_cache_invalidated_after_mutation() {
    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Right,
        1,
    ));
    let rev_before = editor
        .display_map_cache
        .as_ref()
        .map(|(_, rev, _, _)| *rev)
        .unwrap();

    // A mutation increments the buffer revision.
    editor.current_mode = Mode::Insert;
    editor.execute_buffer_command(crate::command::Command::InsertChar('x'));

    let rev_after = editor
        .display_map_cache
        .as_ref()
        .map(|(_, rev, _, _)| *rev)
        .unwrap();

    assert_ne!(
        rev_before, rev_after,
        "mutation must invalidate the display-map cache"
    );
}

#[test]
fn test_resolve_display_map_cached_reuses_across_moves() {
    let mut editor = create_editor_sized(24, 20);
    // Long lines force multi-row wrapping at width ~20.
    set_content(
        &mut editor,
        "this is a fairly long first line that wraps\nsecond long line also wraps here\n",
    );
    let doc_id = editor.document_manager.active_document_id().unwrap();

    let first = editor.resolve_display_map_cached(doc_id, 20);
    let rows_first = first.as_ref().map(|m| m.total_visual_rows());
    assert!(rows_first.unwrap() > 2, "long lines should wrap to >2 rows");

    // A second call with no mutation must hit the cache and return an identical map.
    let cached_rev = editor.display_map_cache.as_ref().map(|(_, r, _, _)| *r);
    let second = editor.resolve_display_map_cached(doc_id, 20);
    assert_eq!(second.map(|m| m.total_visual_rows()), rows_first);
    assert_eq!(
        editor.display_map_cache.as_ref().map(|(_, r, _, _)| *r),
        cached_rev,
        "revision must be unchanged (cache hit, no rebuild)"
    );

    // Equivalence: cached result matches a fresh uncached build.
    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let fresh = super::resolve_display_map(
        doc,
        20,
        editor.state.settings.soft_wrap,
        editor.state.settings.wrap_width,
    );
    assert_eq!(
        editor
            .resolve_display_map_cached(doc_id, 20)
            .map(|m| m.total_visual_rows()),
        fresh.map(|m| m.total_visual_rows()),
    );
}

#[test]
fn test_resolve_display_map_cached_rebuilds_on_tab_width_change() {
    let mut editor = create_editor_sized(24, 20);
    set_content(
        &mut editor,
        "\tindented line that is quite long and wraps\n",
    );
    let doc_id = editor.document_manager.active_document_id().unwrap();

    let before = editor
        .resolve_display_map_cached(doc_id, 20)
        .map(|m| m.tab_width);
    assert_eq!(before, Some(4), "default tab width");

    // Change tab width WITHOUT mutating the buffer (no revision bump). The
    // cached map's stored tab_width no longer matches, so it must rebuild.
    editor
        .document_manager
        .get_document_mut(doc_id)
        .unwrap()
        .options
        .tab_width = 8;
    let after = editor
        .resolve_display_map_cached(doc_id, 20)
        .map(|m| m.tab_width);
    assert_eq!(
        after,
        Some(8),
        "tab-width change must invalidate the cached map"
    );
}

#[test]
fn test_text_changed_coarse_fires_once_per_render() {
    use std::sync::{Arc, Mutex};

    let mut editor = create_editor();

    let count = Arc::new(Mutex::new(0usize));
    let c = count.clone();
    editor
        .plugin_host
        .on("TextChangedCoarse", move |_| *c.lock().unwrap() += 1);

    editor.current_mode = Mode::Insert;
    for _ in 0..5 {
        editor.execute_buffer_command(crate::command::Command::InsertChar('a'));
    }

    assert_eq!(
        *count.lock().unwrap(),
        0,
        "TextChangedCoarse must not fire inside execute_buffer_command"
    );

    // Single render cycle flushes exactly one event.
    editor.update_and_render().unwrap();
    assert_eq!(
        *count.lock().unwrap(),
        1,
        "TextChangedCoarse must fire exactly once per render cycle"
    );

    // A second render with no further mutations must not fire again.
    editor.update_and_render().unwrap();
    assert_eq!(
        *count.lock().unwrap(),
        1,
        "TextChangedCoarse must not fire on a render with no pending changes"
    );
}

#[test]
fn test_cursor_moved_fires_once_per_render_with_latest_position() {
    use std::sync::{Arc, Mutex};

    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");

    let cols: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let c = cols.clone();
    editor.plugin_host.on("CursorMoved", move |event| {
        if let crate::plugin::EditorEvent::CursorMoved { col, .. } = event {
            c.lock().unwrap().push(*col);
        }
    });

    for _ in 0..5 {
        editor.execute_buffer_command(crate::command::Command::Move(
            crate::action::Motion::Right,
            1,
        ));
    }
    assert!(
        cols.lock().unwrap().is_empty(),
        "CursorMoved must not fire inside execute_buffer_command"
    );

    editor.update_and_render().unwrap();
    assert_eq!(
        *cols.lock().unwrap(),
        vec![5],
        "CursorMoved must fire exactly once per render cycle, with the latest position"
    );

    // A second render with no further moves must not fire again.
    editor.update_and_render().unwrap();
    assert_eq!(
        cols.lock().unwrap().len(),
        1,
        "CursorMoved must not fire on a render with no pending moves"
    );
}

#[test]
fn leading_count_composes_with_nest_count_through_full_key_path() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::key::Key;
    use crate::text_objects::Modifier;

    let mut editor = create_editor();
    load_text(&mut editor, "((((ab))))");
    editor.active_document().buffer.set_cursor(4).unwrap();

    // Replay "2di2(" key by key, exactly as run_loop would dispatch it.
    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.current_mode, Mode::OperatorPending);
    assert_eq!(editor.pending_count, 2);

    editor.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
        text_object_input::PendingTextObject::new(Modifier::Inner),
    ));

    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('2'));
    assert!(editor.pending_grammar.is_some());

    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    // Composed nesting (leading 2 * typed 2 = 4) reaches the outermost pair.
    assert_eq!(editor.active_document().buffer.to_string(), "()");
}

#[test]
fn treesitter_object_without_parse_tree_silently_no_ops() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::key::Key;
    use crate::text_objects::Modifier;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(a, b)");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
        text_object_input::PendingTextObject::new(Modifier::Inner),
    ));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('f'));

    assert_eq!(editor.active_document().buffer.to_string(), "foo(a, b)");
    assert!(editor.state.error_manager.notifications().is_empty());
}

#[test]
fn surround_delete_strips_enclosing_parens() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "foobarbaz");
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn surround_delete_via_bracket_alias() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('b'));

    assert_eq!(editor.active_document().buffer.to_string(), "foobarbaz");
}

#[test]
fn surround_change_swaps_parens_for_quotes() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('c'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(editor.active_document().buffer.to_string(), "foo\"bar\"baz");
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn surround_change_pads_opening_bracket_char() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('c'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('{'));

    assert_eq!(editor.active_document().buffer.to_string(), "foo{ bar }baz");
}

#[test]
fn surround_add_wraps_inner_word_with_padding() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::text_objects::Modifier;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar baz");
    editor.active_document().buffer.set_cursor(4).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    assert_eq!(editor.pending_surround_add, Some(1));

    editor.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
        text_object_input::PendingTextObject::new(Modifier::Inner),
    ));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('w'));
    assert_eq!(editor.pending_surround_add, None);

    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "foo ( bar ) baz"
    );
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn surround_add_sgg_wraps_current_line() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\nfoo");
    editor.active_document().buffer.set_cursor(2).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));

    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"hello world\"\nfoo"
    );
}

#[test]
fn surround_delete_dot_repeat_reresolves_at_new_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor.active_document().buffer.set_cursor(1).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));
    assert_eq!(editor.active_document().buffer.to_string(), "a (b)");

    editor.active_document().buffer.set_cursor(3).unwrap();
    editor.execute_dot_repeat();

    assert_eq!(editor.active_document().buffer.to_string(), "a b");
}

#[test]
fn surround_escape_cancels_pending_grammar() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    assert!(editor.pending_grammar.is_some());

    // Mirrors run_loop's Escape-cancels-OperatorPending handling.
    editor.set_mode(Mode::Normal);
    editor.pending_count = 0;
    editor.pending_grammar = None;
    editor.pending_surround_add = None;

    assert!(editor.pending_grammar.is_none());
    let grammar = editor.pending_grammar.take();
    assert!(grammar.is_none());
    // A stray 'j'-like keypress after cancel must not resurrect the surround grammar.
    assert_eq!(editor.active_document().buffer.to_string(), "foo(bar)baz");
    let _ = Key::Char('j');
}

#[test]
fn surround_delete_count_removes_doubled_delimiter() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "((text))");
    editor.active_document().buffer.set_cursor(3).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "text");
}

#[test]
fn surround_change_count_doubles_both_sides() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "\"\"text\"\"");
    editor.active_document().buffer.set_cursor(3).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('c'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "( ( text ) )");
}

#[test]
fn surround_add_count_doubles_delimiter_for_sgg() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "line");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(editor.active_document().buffer.to_string(), "\"\"line\"\"");
}

#[test]
fn surround_add_outer_and_inner_counts_compose_for_sgg() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "line\nline");
    editor.active_document().buffer.set_cursor(0).unwrap();

    // "2s2gg\"": leading 2 (before `s`) doubles the delimiter, inner 2 (typed
    // between the two `g`'s) spans 2 lines, like `2yy`.
    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"\"line\nline\"\""
    );
}

#[test]
fn surround_interrupted_sg_does_not_corrupt_later_yank() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::key::Key;
    use crate::text_objects::Modifier;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar");
    editor.active_document().buffer.set_cursor(0).unwrap();

    // Start `sg` but abandon it by pressing an operator key before supplying
    // a motion, mirroring EditorAction::Operator's reassignment path.
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    assert!(editor.pending_surround_add.is_some());
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.pending_surround_add, None);

    // A different operator key cancels the delete too, returning to a clean
    // Normal-mode state, the same way the run_loop's Escape handler would.
    editor.set_mode(Mode::Normal);
    editor.pending_operator = None;

    // A subsequent plain `yw` must behave as an ordinary yank, not a
    // resurrected surround-add waiting for a delimiter char.
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
        text_object_input::PendingTextObject::new(Modifier::Inner),
    ));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('w'));

    assert_eq!(editor.active_document().buffer.to_string(), "foo bar");
    assert_eq!(editor.current_mode, Mode::Normal);
    assert!(editor.pending_grammar.is_none());
    assert_eq!(ring_text(&editor, 0), Some("foo".to_string()));
}

#[test]
fn surround_give_line_without_pending_add_cancels_like_unrecognized_key() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar");

    // Plain `yg` (no `sg` in progress): pending_surround_add is None, so SurroundGiveLine
    // must cancel back to Normal, not start a phantom surround-add session.
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    editor.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));

    assert_eq!(editor.current_mode, Mode::Normal);
    assert!(editor.pending_operator.is_none());
    assert!(editor.pending_grammar.is_none());
}

#[cfg(feature = "treesitter")]
#[test]
fn stale_syntax_job_result_does_not_clobber_newer_state() {
    use crate::syntax::build_syntax;
    use std::sync::Arc;
    use std::time::Duration;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\n");

    let loader = editor.language_loader.clone();
    let loaded = loader.load_language("rust").expect("rust grammar");
    let highlights = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(loaded, highlights, loader.clone()).expect("build_syntax");
    editor.active_document().set_syntax(syntax);
    editor.do_incremental_syntax_parse();

    let doc_id = editor.active_document_id();

    // Spawn a background reparse, then keep typing before its result arrives --
    // exactly the race that produced offset/flickering highlights (dcf9eaa).
    editor
        .spawn_syntax_parse_job(doc_id)
        .expect("job should spawn");

    {
        let doc = editor.active_document();
        doc.buffer.move_to_end();
        doc.buffer.insert_str("\nfn extra() {}\n").unwrap();
    }

    // Recompute ground truth via a full reparse -- the direct buffer mutation
    // above bypasses `tree.edit()`, so incremental reuse can't be trusted here.
    let source = editor.active_document().buffer.to_logical_bytes();
    let syntax = editor.active_document().syntax.as_mut().unwrap();
    syntax.invalidate_trees();
    assert!(syntax.incremental_parse(&source));
    let fresh_highlights = syntax.highlights(None);
    assert!(!fresh_highlights.is_empty());

    let mut drained = false;
    while let Ok(msg) = editor
        .job_manager
        .receiver()
        .recv_timeout(Duration::from_millis(200))
    {
        editor.handle_job_message(msg).unwrap();
        drained = true;
    }
    assert!(drained, "expected the background job's result to arrive");

    let after_highlights = editor
        .active_document()
        .syntax
        .as_ref()
        .unwrap()
        .highlights(None);
    assert_eq!(
        after_highlights, fresh_highlights,
        "a stale background parse result must not overwrite newer sync-parsed state"
    );
}

#[cfg(feature = "treesitter")]
#[test]
fn undo_keeps_syntax_tree_for_incremental_reuse() {
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\n");

    let loader = editor.language_loader.clone();
    let loaded = loader.load_language("rust").expect("rust grammar");
    let highlights = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(loaded, highlights, loader.clone()).expect("build_syntax");
    editor.active_document().set_syntax(syntax);
    editor.do_incremental_syntax_parse();

    // A real, recorded edit (through the proper Document API, so it both
    // informs the tree via `InputEdit` and is undo-able).
    editor.active_document().insert_str("// comment\n").unwrap();
    editor.do_incremental_syntax_parse();
    assert!(editor
        .active_document()
        .syntax
        .as_ref()
        .unwrap()
        .tree
        .is_some());

    // Undo must keep the tree around for incremental reuse, not discard it --
    // a full reparse forced on every undo is what produced the undo flicker.
    assert!(editor.active_document().undo());
    assert!(
        editor
            .active_document()
            .syntax
            .as_ref()
            .unwrap()
            .tree
            .is_some(),
        "undo must not invalidate the syntax tree"
    );
}

#[cfg(feature = "treesitter")]
#[test]
fn set_aware_delete_keeps_syntax_highlights_in_sync() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::buffer::api::BufferView;
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\nfn extra() {}\n");

    let loader = editor.language_loader.clone();
    let loaded = loader.load_language("rust").expect("rust grammar");
    let highlights_query = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax");
    editor.active_document().set_syntax(syntax);
    editor.do_incremental_syntax_parse();

    // Bank both "fn ... {}" lines via Visual mode (the multi-region path
    // exercised by d/c/y/r/sg/p, not a direct handle-rolled buffer edit).
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualLine));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    let second_line_start = editor.active_document().buffer.line_start(1);
    editor
        .active_document()
        .buffer
        .set_cursor(second_line_start)
        .unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualLine));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.active_document().buffer.to_string(), "");

    let after_set_aware_delete = editor
        .active_document()
        .syntax
        .as_ref()
        .unwrap()
        .highlights(None);

    // Ground truth: force a full reparse of the (now-empty) buffer directly.
    let source = editor.active_document().buffer.to_logical_bytes();
    let syntax = editor.active_document().syntax.as_mut().unwrap();
    syntax.invalidate_trees();
    assert!(syntax.incremental_parse(&source));
    let fresh_highlights = syntax.highlights(None);

    assert_eq!(
        after_set_aware_delete, fresh_highlights,
        "set-aware delete must trigger a sync reparse, not leave the tree stale"
    );
}

#[test]
fn visual_char_enters_mode_and_anchors_at_cursor() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(3).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    assert_eq!(editor.current_mode, Mode::Visual);
    assert_eq!(editor.visual_anchor, Some(3));
}

#[test]
fn visual_resumes_a_banked_region_under_the_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    // Bank a region covering "hello" (0..4), drag direction cursor->anchor
    // reversed (anchor=4, cursor=0) to prove direction is restored exactly.
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 0, RangeKind::Charwise));

    editor.active_document().buffer.set_cursor(2).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    assert_eq!(editor.current_mode, Mode::Visual);
    assert_eq!(
        editor.visual_anchor,
        Some(4),
        "anchor side must be restored"
    );
    assert_eq!(
        editor.active_document().buffer.cursor(),
        0,
        "cursor side must be restored"
    );
    assert!(
        editor.active_document().selection_set.regions.is_empty(),
        "resumed region must be popped out of the banked set"
    );
}

#[test]
fn visual_motion_extends_through_normal_fallthrough() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    assert_eq!(
        editor.current_mode,
        Mode::Visual,
        "motion must not exit Visual"
    );
    assert_eq!(editor.active_document().buffer.cursor(), 2);
    assert_eq!(
        editor.visual_anchor,
        Some(0),
        "anchor stays fixed while cursor moves"
    );
}

#[test]
fn visual_swap_ends_exchanges_anchor_and_cursor() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    // anchor=0, cursor=2

    editor.handle_action(&Action::Editor(EditorAction::VisualSwapEnds));

    assert_eq!(editor.visual_anchor, Some(2));
    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn expand_region_grows_word_then_quotes_in_verified_order() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "say \"hello world\" now");
    let pos = editor
        .active_document()
        .buffer
        .to_string()
        .find("hello")
        .unwrap();
    editor.active_document().buffer.set_cursor(pos).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let anchor = editor.visual_anchor.unwrap();
    let cursor = editor.active_document().buffer.cursor();
    assert_eq!(
        (anchor, cursor),
        (5, 10),
        "first press: Word around -> \"hello \""
    );

    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let anchor = editor.visual_anchor.unwrap();
    let cursor = editor.active_document().buffer.cursor();
    assert_eq!(
        (anchor, cursor),
        (4, 16),
        "second press: DoubleQuote around -> the full quoted span"
    );
}

#[test]
fn expand_region_noop_when_already_at_buffer_extent() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "x");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    // Expand repeatedly until it stops growing (terminates quickly on a
    // 1-char buffer); the last call must leave anchor/cursor unchanged.
    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let before = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );
    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let after = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );

    assert_eq!(
        before, after,
        "expanding past the whole buffer must be a no-op"
    );
}

#[test]
fn shrink_region_pops_the_last_expand_step() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "say \"hello world\" now");
    let pos = editor
        .active_document()
        .buffer
        .to_string()
        .find("hello")
        .unwrap();
    editor.active_document().buffer.set_cursor(pos).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    let before_expand = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );
    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let after_expand = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );
    assert_ne!(
        before_expand, after_expand,
        "expand must have actually grown the region"
    );

    editor.handle_action(&Action::Editor(EditorAction::ShrinkRegion));
    let after_shrink = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );

    assert_eq!(
        after_shrink, before_expand,
        "shrink must restore the exact pre-expand extent"
    );
}

#[test]
fn shrink_region_with_empty_history_is_a_noop() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    let handled = editor.handle_action(&Action::Editor(EditorAction::ShrinkRegion));

    assert!(!handled);
}

#[test]
fn escape_in_visual_commits_active_region() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.current_mode, Mode::Normal);
    assert!(editor.visual_anchor.is_none());
    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
    assert_eq!(
        editor.active_document().selection_set.regions[0].span(),
        (0, 3)
    );
}

#[test]
fn escape_in_normal_clears_a_nonempty_banked_set() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn issue_worked_example_bank_two_regions_no_delete_yet() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "Hello\nworld\nfoo\n");

    // goto line 1 (already there) -> v -> select "Ho" -> Esc
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
    assert_eq!(
        editor.active_document().selection_set.regions[0].span(),
        (0, 2)
    );

    // goto line 3 (plain motion, set untouched) -> v -> select "f" -> Esc
    let line3_start = editor.active_document().buffer.line_start(2);
    let _ = editor.active_document().buffer.set_cursor(line3_start);
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        2,
        "first region must survive the plain motion to line 3"
    );
    let spans: Vec<(usize, usize)> = editor
        .active_document()
        .selection_set
        .sorted()
        .iter()
        .map(|r| r.span())
        .collect();
    assert_eq!(spans[0], (0, 2), "\"Ho\" region");
    assert_eq!(spans[1].1 - spans[1].0, 1, "\"f\" region is one char");
}

#[test]
fn n_cycles_banked_regions_when_set_is_nonempty() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789abcdefghij");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(10, 11, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 10);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(
        editor.active_document().buffer.cursor(),
        0,
        "wraps to first"
    );
}

#[test]
fn shift_n_cycles_backward_when_set_is_nonempty() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789abcdefghij");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(10, 11, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(15).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 10);
}

#[test]
fn n_keeps_repeat_find_behavior_when_set_is_empty() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo baz");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.state.last_find_char = Some(('o', true, false)); // as if `fo` had just run

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));

    // Repeat-find-char behavior, completely untouched: lands on the next 'o'.
    assert_eq!(editor.active_document().buffer.cursor(), 1);
}

#[test]
fn region_bank_occurrence_next_finds_and_moves_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo baz foo");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert_eq!(editor.active_document().selection_set.regions.len(), 2);
    assert_eq!(editor.active_document().buffer.cursor(), 8);
}

#[test]
fn region_bank_occurrence_on_empty_set_is_a_noop() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");

    let handled = editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert!(!handled);
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn region_bank_occurrence_disabled_for_blockwise_last_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Blockwise));

    let handled = editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert!(!handled);
    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
}

#[test]
fn apply_to_each_region_runs_f_once_per_region_highest_offset_first() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    let mut seen_starts = Vec::new();
    let handled = editor.apply_to_each_region(|_editor, region| {
        seen_starts.push(region.span().0);
        true
    });

    assert!(handled);
    assert_eq!(seen_starts, vec![5, 0], "highest-offset-first");
    assert!(
        editor.active_document().selection_set.is_empty(),
        "batch must clear the set"
    );
}

#[test]
fn apply_to_each_region_on_empty_set_returns_false() {
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");

    let handled = editor.apply_to_each_region(|_editor, _region| true);

    assert!(!handled);
}

#[test]
fn apply_to_each_region_deletes_are_one_undo_step() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    editor.apply_to_each_region(|editor, region| {
        let (start, end) = region.span();
        if let Some(doc) = editor.document_manager.active_document_mut() {
            doc.delete_range(start, end).is_ok()
        } else {
            false
        }
    });
    assert_eq!(editor.active_document().buffer.to_string(), "234789");

    assert!(editor.active_document().undo());
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "0123456789",
        "a single undo must restore both deletions at once"
    );
}

#[test]
fn enter_multi_insert_replays_typed_session_at_every_remaining_anchor() {
    use crate::action::{Action, EditorAction};
    use crate::command::Command;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    let handled =
        editor.enter_multi_insert(Command::EnterInsertMode, |_doc, region| region.span().0);
    assert!(handled);
    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(
        editor.active_document().buffer.cursor(),
        5,
        "starts at the highest-offset anchor"
    );

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "X01234X56789",
        "X inserted at both original anchors: live at 5, replayed at 0"
    );
    assert!(editor.pending_multi_insert_anchors.is_empty());
}

#[test]
fn enter_multi_insert_on_empty_set_returns_false() {
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");

    let handled = editor
        .enter_multi_insert(crate::command::Command::EnterInsertMode, |_doc, region| {
            region.span().0
        });

    assert!(!handled);
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn set_aware_delete_removes_every_banked_region_as_one_op() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    // Bank "foo" (0..2) and the two touching "foo"s inside "foofoo" (5..7, 8..10).
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 7, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 10, RangeKind::Charwise));
    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        3,
        "touching must not have merged"
    );

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "\n\n\n");
    assert!(
        editor.active_document().selection_set.is_empty(),
        "set clears after the batch"
    );
}

#[test]
fn set_aware_delete_is_one_undo_step() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.active_document().buffer.to_string(), "234789");

    assert!(editor.active_document().undo());
    assert_eq!(editor.active_document().buffer.to_string(), "0123456789");
}

#[test]
fn set_aware_yank_captures_each_region_without_mutating() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar baz");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 10, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "foo bar baz",
        "yank must not mutate"
    );
    assert!(editor.active_document().selection_set.is_empty());
    assert_eq!(
        ring_text(&editor, 0),
        Some("foo".to_string()),
        "lowest-offset region pushed last = ring[0] (front-insert)"
    );
    assert_eq!(ring_text(&editor, 1), Some("baz".to_string()));
}

#[test]
fn visual_d_commits_active_region_then_runs_the_batch() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    // active region now 0..1 (chars "0","1"), banked set still has 5..6 ("5")

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "234789",
        "both the just-committed and pre-banked region deleted as one batch"
    );
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn plain_d_with_empty_set_is_unaffected() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(
        editor.current_mode,
        Mode::OperatorPending,
        "falls through to today's single-cursor flow"
    );
}

#[test]
fn canonical_change_across_touching_regions_does_not_merge_them() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 7, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 10, RangeKind::Charwise));
    assert_eq!(editor.active_document().selection_set.regions.len(), 3);

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Change,
    )));
    assert_eq!(editor.current_mode, Mode::Insert);

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('b')));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('a')));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('r')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "bar\n\nbarbar\n",
        "each touching region gets its own independent 'bar', not one merged replacement"
    );
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn single_region_change_unaffected_by_the_new_batching_branch() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Change,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::NextWord)));
    assert_eq!(
        editor.current_mode,
        Mode::Insert,
        "ordinary single-cursor cw must still work"
    );

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "Xbar");
}

#[test]
fn multi_i_inserts_at_start_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    assert_eq!(editor.current_mode, Mode::Insert);
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "X01234X56789");
}

#[test]
fn multi_a_inserts_after_end_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAfter));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "0X12345X6789");
}

#[test]
fn multi_capital_i_inserts_at_line_start_of_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    // region inside "bbb" (offset 5, the second 'b') and inside "ccc" (offset 9)
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(9, 9, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineStart));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "aaa\nXbbb\nXccc"
    );
}

#[test]
fn multi_capital_a_inserts_at_line_end_of_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 4, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 8, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineEnd));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "aaa\nbbbX\ncccX"
    );
}

#[test]
fn multi_o_opens_a_new_line_below_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::OpenLineBelow));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "aaa\nX\nbbb\nX\nccc"
    );
}

#[test]
fn multi_capital_o_opens_a_new_line_above_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::OpenLineAbove));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "X\naaa\nX\nbbb\nccc"
    );
}

#[test]
fn plain_i_with_empty_set_is_unaffected() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello");
    editor.active_document().buffer.set_cursor(2).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(
        editor.active_document().buffer.cursor(),
        2,
        "ordinary i must still anchor at the live cursor"
    );
}

#[test]
fn set_aware_replace_char_fills_each_region_to_its_own_length() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise)); // len 2
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 8, RangeKind::Charwise)); // len 4

    editor.handle_action(&Action::Editor(EditorAction::ReplaceCharPending));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('x'));

    assert_eq!(editor.active_document().buffer.to_string(), "xx234xxxx9");
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_sd_strips_surrounding_parens_from_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(1, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "a b");
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_sg_wraps_each_region_independently() {
    use crate::action::Motion;
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 7, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 10, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::AddSurroundChar {
        motion: Motion::NextWord,
        count: 1,
        delim_count: 1,
    });
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"foo\"\n\n\"foo\"\"foo\"\n"
    );
}

#[test]
fn set_aware_put_inserts_same_text_at_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "0X12345X6789",
        "p inserts after each region"
    );
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_put_before_inserts_ahead_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: true }));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "X01234X56789",
        "P inserts before each region"
    );
}

#[test]
fn bare_repeated_p_after_set_aware_put_only_affects_single_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    let count_of_x = editor
        .active_document()
        .buffer
        .to_string()
        .matches('X')
        .count();
    assert_eq!(
        count_of_x, 3,
        "first put = 2 X's (one per region), second bare put = 1 more, not 2 more"
    );
}

#[test]
fn multi_insert_is_one_undo_step() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "X01234X56789");

    assert!(editor.active_document().undo());
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "0123456789",
        "a single undo must remove both inserted X's at once"
    );
}

#[test]
fn region_build_actions_accumulate_while_visual_and_during_bank_occurrence() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert_eq!(
        editor.region_build_recording,
        vec![
            Action::Editor(EditorAction::EnterVisualChar),
            Action::Editor(EditorAction::Move(crate::action::Motion::Right)),
            Action::Editor(EditorAction::Move(crate::action::Motion::Right)),
            Action::Editor(EditorAction::EnterNormalMode),
            Action::Editor(EditorAction::RegionBankOccurrenceNext),
        ]
    );
}

#[test]
fn region_build_recording_does_not_capture_plain_normal_mode_navigation() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "line one\nline two\nline three");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    // Plain navigation between bank operations is not part of the
    // recorded sequence; "." rebuilds relative to the cursor's new position.
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));

    assert_eq!(
        editor.region_build_recording,
        vec![
            Action::Editor(EditorAction::EnterVisualChar),
            Action::Editor(EditorAction::EnterNormalMode),
        ],
        "plain Normal-mode Move actions must not be recorded"
    );
}

#[test]
fn multi_region_put_is_one_undo_step() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    assert_eq!(editor.active_document().buffer.to_string(), "0X12345X6789");

    assert!(editor.active_document().undo());
    assert_eq!(editor.active_document().buffer.to_string(), "0123456789");
}

#[test]
fn dot_repeat_destructive_group_reselects_without_reexecuting() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.active_document().buffer.to_string(), " (b)");
    assert!(editor.active_document().selection_set.is_empty());

    editor.active_document().buffer.set_cursor(1).unwrap(); // land inside "(b)"
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        " (b)",
        "destructive group: '.' must NOT re-delete"
    );
    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        1,
        "but the equivalent region must be rebanked for manual review"
    );
}

#[test]
fn dd_with_leading_count_deletes_n_lines() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "one\ntwo\nthree\nfour\n");

    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "four\n");
}

#[test]
fn dd_count_does_not_leak_into_next_motion() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "one\ntwo\nthree\nfour\nfive\n");

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "three\nfour\nfive\n"
    );
    assert_eq!(editor.pending_count, 0, "count must not survive past dd");

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));
    assert_eq!(
        editor.active_document().buffer.get_line(),
        1,
        "a leaked count would have moved down 2 lines instead of 1"
    );
}

#[test]
fn x_with_leading_count_deletes_n_chars() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Delete(Motion::Right)));

    assert_eq!(editor.active_document().buffer.to_string(), "lo world");
}

#[test]
fn operator_count_and_motion_count_multiply_not_concatenate() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "one two three four five six seven eight");
    editor.active_document().buffer.set_cursor(0).unwrap();

    // "2d3w" must delete 2*3=6 words, not loop a 23-word delete; mirrors
    // run_loop's digit handler stashing the operator count for the motion.
    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_operator_count = editor.pending_count.max(1);
    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::NextWord)));

    assert_eq!(editor.active_document().buffer.to_string(), "seven eight");
}

#[test]
fn dot_repeat_leading_count_overrides_embedded_command_count() {
    use crate::action::Motion;
    use crate::command::Command;

    let mut editor = create_editor();
    load_text(&mut editor, "one two three four five six seven");
    editor.active_document().buffer.set_cursor(0).unwrap();

    // Simulate "d2w" having just run and been recorded for dot-repeat.
    let d2w = Command::Delete(Motion::NextWord, 2);
    editor.execute_buffer_command(d2w);
    editor.dot_repeat.record_single(d2w);
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "three four five six seven"
    );

    // "3." must run d3w ONCE (vim: leading count replaces the embedded
    // count), not loop the original 2-word delete 3 times (6 words).
    editor.pending_count = 3;
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "six seven",
        "3. after d2w must delete 3 words once, not 2 words three times"
    );
}

#[test]
fn dot_repeat_non_destructive_group_rebuilds_and_reexecutes() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineStart));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "Xaaa\nbbb");

    let bbb_offset = editor
        .active_document()
        .buffer
        .to_string()
        .find('b')
        .unwrap();
    editor
        .active_document()
        .buffer
        .set_cursor(bbb_offset)
        .unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "Xaaa\nXbbb",
        "non-destructive group: '.' rebuilds AND re-runs the insert"
    );
}

#[test]
fn dot_repeat_sg_fully_replays_using_addsurroundtoset() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "foo\nbar");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode)); // banks "foo" (0..2)

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, crate::key::Key::Char('g'));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::AddSurroundChar {
        motion: Motion::NextWord, // ignored by the set-aware path
        count: 1,
        delim_count: 1,
    });
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, crate::key::Key::Char('"'));

    assert_eq!(editor.active_document().buffer.to_string(), "\"foo\"\nbar");

    let bar_offset = editor
        .active_document()
        .buffer
        .to_string()
        .find("bar")
        .unwrap();
    editor
        .active_document()
        .buffer
        .set_cursor(bar_offset)
        .unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"foo\"\n\"bar\"",
        "'.' rebuilds the equivalent region at the new cursor AND re-wraps it -- sg fully replays, unlike d/c/y/sd/sc"
    );
}

#[test]
fn gv_toggle_opens_and_closes_regardless_of_focus() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    assert!(editor.panel_layout.is_some(), "gv opens the window");
    assert_eq!(
        editor.active_document().kind.kind_str(),
        "regions",
        "focus moves into the new regions window"
    );

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    assert!(editor.panel_layout.is_none(), "a second gv closes it again");
}

#[test]
fn gv_with_empty_set_does_not_open_a_window() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    assert!(editor.panel_layout.is_none());
}

#[test]
fn regions_window_x_drops_the_selected_entry() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    editor.handle_action(&Action::Editor(EditorAction::RegionsListDrop));

    let source_id = match editor.active_document().kind {
        crate::document::BufferKind::Regions { source_doc_id } => source_doc_id,
        _ => panic!("expected to still be focused in the regions window"),
    };
    assert_eq!(
        editor
            .document_manager
            .get_document(source_id)
            .unwrap()
            .selection_set
            .regions
            .len(),
        1,
        "one entry dropped from the *source* document's set"
    );
}

#[test]
fn regions_window_j_moves_the_list_cursor_and_live_jumps_the_preview() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));
    let source_id = editor.active_document_id();
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    let list_cursor_before = editor.active_document().buffer.cursor();

    editor.handle_action(&Action::Editor(EditorAction::RegionsListDown));

    assert_ne!(
        editor.active_document().buffer.cursor(),
        list_cursor_before,
        "j must move the regions list's own cursor to line 2, not stay on line 1"
    );
    assert_eq!(
        editor
            .document_manager
            .get_document(source_id)
            .unwrap()
            .buffer
            .cursor(),
        5,
        "and live-jump the source buffer to the second region (sorted order: 0..1, then 5..6)"
    );
}

#[test]
fn regions_window_operator_redirects_to_the_source_document() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));
    let source_id = editor.active_document_id();
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert!(
        editor.panel_layout.is_none(),
        "firing an operator from the window closes it"
    );
    assert_eq!(
        editor
            .document_manager
            .get_document(source_id)
            .unwrap()
            .buffer
            .to_string(),
        "234789"
    );
}

#[test]
fn visual_block_renders_and_edits_identically_to_charwise() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualBlock));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "23456789",
        "Ctrl-V behaves exactly like v -- no rectangle semantics by design"
    );
}

#[test]
fn issue_worked_example_full_sequence_including_delete() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "Hello\nworld\nfoo\n");

    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode)); // banks "He"

    let line3_start = editor.active_document().buffer.line_start(2);
    let _ = editor.active_document().buffer.set_cursor(line3_start);
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode)); // banks "f"

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "llo\nworld\noo\n"
    );
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn undo_of_unrelated_edit_clears_a_banked_set() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().insert_char('!').unwrap(); // an edit not routed through any driver
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    assert!(!editor.active_document().selection_set.is_empty());

    assert!(editor.active_document().undo());

    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn every_set_aware_command_clears_the_set_after_acting() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let fresh_set = |editor: &mut Editor<MockTerminal>| {
        editor
            .active_document()
            .selection_set
            .bank(Region::new(0, 0, RangeKind::Charwise));
        editor
            .active_document()
            .selection_set
            .bank(Region::new(4, 4, RangeKind::Charwise));
    };

    // d
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "d must clear the set"
    );

    // y
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "y must clear the set"
    );

    // i (then Esc to finish the insert session)
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "i must clear the set"
    );

    // o
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::OpenLineBelow));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "o must clear the set"
    );

    // p
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "p must clear the set"
    );
}

#[test]
fn dot_repeat_yank_reselects_without_reexecuting() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    let buffer_before = editor.active_document().buffer.to_string();
    assert!(editor.active_document().selection_set.is_empty());

    editor.active_document().buffer.set_cursor(5).unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        buffer_before,
        "yank's dot-repeat must not mutate the buffer"
    );
    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        1,
        "but must rebank the equivalent region"
    );
}

// Builds the set via `m` (RegionBankOccurrenceNext) on a repeated
// substring so the build sequence is recorded and replays at a new cursor.
#[test]
fn dot_repeat_paste_genuinely_differs_from_bare_repeat() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo baz foo");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode)); // banks "foo" at 0..2
    editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext)); // banks "foo" at 8..10

    editor.clipboard_ring.push_str("X".to_string());
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false })); // 1st X after each anchor

    let _ = editor.active_document().buffer.set_cursor(0);
    editor.execute_dot_repeat(); // rebuild relative to cursor 0 + re-run: 2nd X at each
    let _ = editor.active_document().buffer.set_cursor(0);
    editor.execute_dot_repeat(); // 3rd X at each

    let count_of_x = editor
        .active_document()
        .buffer
        .to_string()
        .matches('X')
        .count();
    assert_eq!(
        count_of_x, 6,
        "three dot-repeats x two original anchors = 6, not stacked at one spot"
    );
}

// The set-aware multi-region Put path never establishes `post_paste_state`,
// so CyclePaste has no single position to act on and correctly no-ops.
#[test]
fn cycle_paste_after_set_clears_only_touches_the_single_most_recent_position() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("Y".to_string());
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false })); // set clears here
    editor.handle_action(&Action::Editor(EditorAction::CyclePaste { forward: true }));

    let count_of_y = editor
        .active_document()
        .buffer
        .to_string()
        .matches('Y')
        .count();
    assert_eq!(
        count_of_y, 0,
        "multi-region put never sets post_paste_state, so CyclePaste correctly no-ops"
    );
}

#[test]
fn real_keymap_v_then_l_renders_a_visible_highlight_in_the_composited_cells() {
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    let feed_key = |editor: &mut Editor<MockTerminal>, key: Key| {
        let context = if editor.current_mode.is_visual() {
            KeyContext::Visual
        } else {
            KeyContext::Normal
        };
        match editor.keymap.lookup(context, &[key]) {
            MatchResult::Exact(action) | MatchResult::Ambiguous(action) => {
                let action = action.clone();
                editor.handle_action(&action);
            }
            other => panic!("key {key:?} in context {context:?} did not resolve: {other:?}"),
        }
    };

    // Drive through the real keymap (not handle_action directly) so this
    // catches keymap/context wiring gaps, not just annotation logic.
    feed_key(&mut editor, Key::Char('v'));
    feed_key(&mut editor, Key::Char('l'));

    editor.update_and_render().unwrap();

    let cols = editor.render_system.compositor.cols();
    let cells = editor.render_system.compositor.get_composited_slice();
    let highlighted = cells[..cols]
        .iter()
        .filter(|c| {
            matches!(
                c.bg,
                Some(crate::color::Color::Rgb {
                    r: 100,
                    g: 160,
                    b: 220
                })
            )
        })
        .count();
    assert_eq!(
        highlighted, 2,
        "v then l must highlight exactly the 2 selected chars 'h','e'"
    );
}

#[test]
fn visual_highlight_redraws_on_a_frame_after_the_initial_one() {
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    // Render once first, mirroring the real run loop's initial-open render --
    // ContentDrawState's redraw check only catches an annotation-only change
    // (no buffer edit, no scroll) if something hashes the annotation spans.
    editor.update_and_render().unwrap();

    let feed_key = |editor: &mut Editor<MockTerminal>, key: Key| {
        let context = if editor.current_mode.is_visual() {
            KeyContext::Visual
        } else {
            KeyContext::Normal
        };
        match editor.keymap.lookup(context, &[key]) {
            MatchResult::Exact(action) | MatchResult::Ambiguous(action) => {
                let action = action.clone();
                editor.handle_action(&action);
            }
            other => panic!("key {key:?} did not resolve: {other:?}"),
        }
    };

    feed_key(&mut editor, Key::Char('v'));
    feed_key(&mut editor, Key::Char('l'));

    editor.update_and_render().unwrap();

    let cols = editor.render_system.compositor.cols();
    let cells = editor.render_system.compositor.get_composited_slice();
    let highlighted = cells[..cols]
        .iter()
        .filter(|c| {
            matches!(
                c.bg,
                Some(crate::color::Color::Rgb {
                    r: 100,
                    g: 160,
                    b: 220
                })
            )
        })
        .count();
    assert_eq!(
        highlighted, 2,
        "selection highlight must redraw on a later frame, not just the first ever render"
    );
}
