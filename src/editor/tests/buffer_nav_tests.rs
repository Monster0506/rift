use super::common::*;
use super::*;
use crate::document::ops::{Edit, EditSource};
use crate::error::ErrorSeverity;

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

    let result = editor.close_buffer(doc_id, crate::document::manager::RemovalIntent::Normal);
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
    editor
        .active_document()
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap();
    let doc_id = editor.document_manager.get_document_id_at(0).unwrap();

    let result = editor.close_buffer(doc_id, crate::document::manager::RemovalIntent::Normal);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.severity, ErrorSeverity::Warning);
}

#[test]
fn rejected_close_has_no_cleanup_side_effects() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CLOSE_COUNT: AtomicUsize = AtomicUsize::new(0);

    fn count_close(_handle: crate::document::runtime::DocumentHandle) {
        CLOSE_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    CLOSE_COUNT.store(0, Ordering::SeqCst);
    let mut editor = create_editor();
    let doc_id = editor.active_document_id();
    let mut descriptor = (*crate::document::runtime::builtin_descriptor(
        crate::document::runtime::BufferKindId::FILE,
    ))
    .clone();
    descriptor.on_close = crate::document::runtime::CloseHandler::Native(count_close);
    editor.active_document().kind =
        crate::document::kind::BufferKind::new(std::sync::Arc::new(descriptor));
    editor
        .active_document()
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap();

    assert!(editor
        .close_buffer(doc_id, crate::document::manager::RemovalIntent::Normal)
        .is_err());
    assert_eq!(CLOSE_COUNT.load(Ordering::SeqCst), 0);
    assert!(editor.document_manager.get_document(doc_id).is_some());

    editor
        .close_buffer(doc_id, crate::document::manager::RemovalIntent::Force)
        .unwrap();
    assert_eq!(CLOSE_COUNT.load(Ordering::SeqCst), 1);
    assert!(editor.document_manager.get_document(doc_id).is_none());
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
    editor
        .active_document()
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap();
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
fn q_key_refuses_dirty_buffer_when_quitonq_off() {
    let mut editor = create_editor();
    editor.execute_command_line("set noquitonq");
    editor
        .active_document()
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap();
    assert!(editor.active_document().is_dirty());

    feed_keys(&mut editor, "q");

    assert!(!editor.should_quit, "q must honor the dirty check");
}

#[test]
fn q_key_quits_dirty_buffer_by_default() {
    let mut editor = create_editor();
    editor
        .active_document()
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap();
    assert!(editor.active_document().is_dirty());

    feed_keys(&mut editor, "q");

    assert!(editor.should_quit);
}

#[test]
fn q_key_closes_split_when_quitonq_off() {
    let mut editor = create_editor();
    editor.execute_command_line("set noquitonq");
    do_vsplit(&mut editor);
    assert_eq!(editor.split_tree.window_count(), 2);

    feed_keys(&mut editor, "q");

    assert!(!editor.should_quit);
    assert_eq!(editor.split_tree.window_count(), 1);
}

#[test]
fn test_handle_execution_result_quit_only_checks_current_buffer() {
    let mut editor = create_editor();

    editor
        .active_document()
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap();
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
fn test_cancelled_save_clears_quit_after_job() {
    let mut editor = create_editor();
    let job_id = 42;
    editor.scheduler.quit_after_job = Some(job_id);

    editor.handle_job_message(crate::job_manager::JobMessage::Cancelled(job_id));

    assert_eq!(
        editor.scheduler.quit_after_job, None,
        "a cancelled save tied to a pending quit must clear quit_after_job"
    );
    assert!(
        !editor.should_quit,
        "a cancelled save must not force the editor to quit"
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
fn test_buffer_goto_jumps_by_b_list_index() {
    let mut editor = create_editor();
    editor
        .open_file(Some("doc1.txt".to_string()), false)
        .unwrap();
    editor
        .open_file(Some("doc2.txt".to_string()), false)
        .unwrap();
    assert_eq!(editor.document_manager.active_tab_index(), 2);

    editor.execute_command_line("b 1");
    assert_eq!(editor.document_manager.active_tab_index(), 0);
    assert_eq!(editor.active_document().display_name(), "[No Name]");

    editor.execute_command_line("buffer 3");
    assert_eq!(editor.document_manager.active_tab_index(), 2);
    assert_eq!(editor.active_document().display_name(), "doc2.txt");

    editor.execute_command_line("b 99");
    assert_eq!(editor.document_manager.active_tab_index(), 2);
}

#[test]
fn test_bdelete_removes_current_buffer_and_shows_neighbor() {
    let mut editor = create_editor();
    editor
        .open_file(Some("doc1.txt".to_string()), false)
        .unwrap();
    editor
        .open_file(Some("doc2.txt".to_string()), false)
        .unwrap();
    assert_eq!(editor.document_manager.tab_count(), 3);

    editor.execute_command_line("bdelete");
    assert_eq!(editor.document_manager.tab_count(), 2);
    assert_eq!(editor.active_document().display_name(), "doc1.txt");
}

#[test]
fn test_bd_by_index_deletes_a_non_current_buffer() {
    let mut editor = create_editor();
    editor
        .open_file(Some("doc1.txt".to_string()), false)
        .unwrap();
    editor
        .open_file(Some("doc2.txt".to_string()), false)
        .unwrap();
    assert_eq!(editor.document_manager.active_tab_index(), 2);

    editor.execute_command_line("bd 2");
    assert_eq!(editor.document_manager.tab_count(), 2);
    assert_eq!(editor.active_document().display_name(), "doc2.txt");
    assert_eq!(editor.document_manager.active_tab_index(), 1);
}

#[test]
fn test_bdelete_refuses_dirty_buffer_without_bang() {
    let mut editor = create_editor();
    editor
        .active_document()
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap();
    assert_eq!(editor.document_manager.tab_count(), 1);

    editor.execute_command_line("bdelete");
    assert_eq!(
        editor.document_manager.tab_count(),
        1,
        "dirty buffer must survive a bare :bdelete"
    );

    editor.execute_command_line("bdelete!");
    assert_eq!(editor.active_document().buffer().to_string(), "");
}

#[test]
fn test_open_buffer_list_panel_shows_status_and_enter_switches() {
    let mut editor = create_editor();
    editor
        .open_file(Some("doc1.txt".to_string()), false)
        .unwrap();
    editor
        .open_file(Some("doc2.txt".to_string()), false)
        .unwrap();
    editor
        .active_document()
        .edit(Edit::type_text('x'), EditSource::User)
        .unwrap();
    let doc1_id = editor.document_manager.get_document_id_at(1).unwrap();

    editor.execute_command_line("buffer");

    let layout = editor
        .panel_layout_of(crate::editor::PanelKind::BufferList)
        .expect(":buffer with no args opens the buffer-list split panel");
    let list_doc = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .unwrap();
    let text = list_doc.buffer().to_string();
    assert!(text.contains("[2] doc1.txt:  "));
    assert!(text.contains("[3] doc2.txt: %+"));

    {
        let doc = editor
            .document_manager
            .get_document_mut(layout.dir_doc_id)
            .unwrap();
        let line_start = doc.buffer().line_index.get_start(1).unwrap();
        let _ = doc.set_cursor(line_start, crate::document::ops::CursorCause::User);
    }
    editor.handle_buffer_list_select();

    assert!(editor
        .panel_layout_of(crate::editor::PanelKind::BufferList)
        .is_none());
    assert_eq!(editor.document_manager.active_document_id(), Some(doc1_id));
}

#[test]
fn test_buffer_list_j_snaps_between_entries_and_updates_preview() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    editor
        .open_file(Some("doc1.txt".to_string()), false)
        .unwrap();
    editor
        .open_file(Some("doc2.txt".to_string()), false)
        .unwrap();
    let doc1_id = editor.document_manager.get_document_id_at(1).unwrap();

    editor.execute_command_line("buffer");
    let layout = editor
        .panel_layout_of(crate::editor::PanelKind::BufferList)
        .unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));
    let preview_shown = editor
        .split_tree
        .get_window(layout.preview_win_id)
        .unwrap()
        .document_id;
    assert_eq!(preview_shown, doc1_id);

    let before = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .unwrap()
        .buffer()
        .to_string();
    editor
        .active_document()
        .edit(Edit::type_text('z'), EditSource::User)
        .unwrap();
    let after = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .unwrap()
        .buffer()
        .to_string();
    assert_eq!(before, after, "buffer-list panel must be non-editable");
}

#[test]
fn test_read_only_buffer_refuses_to_enter_insert_mode() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    editor
        .open_file(Some("doc1.txt".to_string()), false)
        .unwrap();
    editor.execute_command_line("buffer");
    assert!(editor
        .panel_layout_of(crate::editor::PanelKind::BufferList)
        .is_some());

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    assert_eq!(editor.mode(), Mode::Normal);

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAfter));
    assert_eq!(editor.mode(), Mode::Normal);

    editor.handle_action(&Action::Editor(EditorAction::OpenLineBelow));
    assert_eq!(editor.mode(), Mode::Normal);

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        crate::action::OperatorType::Change,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    assert_eq!(editor.mode(), Mode::Normal);
}

#[test]
fn test_search_closes_on_success() {
    let mut editor = create_editor();

    editor
        .open_file(Some("test.txt".to_string()), false)
        .unwrap();
    editor
        .active_document()
        .replace_content("hello world", crate::document::ops::ContentKind::Render);

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
        .replace_content("hello world", crate::document::ops::ContentKind::Render);

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
