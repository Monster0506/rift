use super::common::*;

#[test]
fn splits_of_one_buffer_restore_their_own_cursor_positions() {
    let mut editor = editor_with_text("abcdefgh");
    let left = editor.split_tree.focused_window_id();
    let cause = crate::document::ops::CursorCause::User;
    editor.active_document().set_cursor(3, cause).unwrap();
    let doc = editor.active_document_id();
    let right = editor
        .split_window(crate::split::tree::SplitDirection::Vertical, doc)
        .unwrap();
    assert_eq!(editor.active_document().buffer().cursor(), 3);

    editor.active_document().set_cursor(6, cause).unwrap();
    editor.focus_window(left);
    assert_eq!(editor.active_document().buffer().cursor(), 3);
    editor.focus_window(right);
    assert_eq!(editor.active_document().buffer().cursor(), 6);
}

#[test]
fn exchanging_split_panes_restores_each_cursors_position_for_a_shared_buffer() {
    use crate::document::ops::CursorCause;
    use crate::split::tree::SplitDirection;

    let mut editor = editor_with_text("abcdefgh");
    let left = editor.split_tree.focused_window_id();
    let doc = editor.active_document_id();
    editor
        .active_document()
        .set_cursor(2, CursorCause::User)
        .unwrap();
    let right = editor.split_window(SplitDirection::Vertical, doc).unwrap();
    editor
        .active_document()
        .set_cursor(5, CursorCause::User)
        .unwrap();

    assert!(editor.swap_windows());
    assert_eq!(editor.split_tree.focused_window_id(), left);
    assert_eq!(editor.active_document().buffer().cursor(), 2);
    editor.focus_previous_window();
    assert_eq!(editor.split_tree.focused_window_id(), right);
    assert_eq!(editor.active_document().buffer().cursor(), 5);
}

#[test]
fn page_motions_use_the_height_of_the_focused_split() {
    let mut editor = editor_with_text(&"line\n".repeat(80));
    editor.execute_command_line("split");
    editor.update_and_render().unwrap();
    let page = editor.focused_viewport().visible_rows();
    assert!(page < editor.render_system.viewport.visible_rows());

    press(&mut editor, &keys("<PageDown>"));
    assert_eq!(editor.active_document().buffer().get_line(), page);
    press(&mut editor, &keys("<PageDown><PageUp>"));
    assert_eq!(editor.active_document().buffer().get_line(), page);
}

#[test]
fn showing_a_buffer_again_preserves_its_cursor_update_while_unfocused() {
    use crate::document::{manager::Placement, Document};

    let mut editor = editor_with_text("abcdefgh");
    let left = editor.split_tree.focused_window_id();
    let original = editor.active_document_id();
    let id = editor.document_manager.next_id();
    editor.open_buffer(Document::new(id).unwrap(), Placement::Background);
    editor
        .split_window(crate::split::tree::SplitDirection::Vertical, id)
        .unwrap();
    editor
        .document_manager
        .get_document_mut(original)
        .unwrap()
        .set_cursor(4, crate::document::ops::CursorCause::User)
        .unwrap();

    editor.focus_window(left);

    assert_eq!(editor.active_document().buffer().cursor(), 4);
}

#[test]
fn editing_targets_the_buffer_shown_by_the_focused_split() {
    use crate::document::{manager::Placement, ops::ContentKind, Document};

    let mut editor = editor_with_text("original");
    editor.execute_command_line("vsplit");
    let original = editor.active_document_id();
    let id = editor.document_manager.next_id();
    let mut preview = Document::new(id).unwrap();
    preview.replace_content("preview", ContentKind::Load);
    editor.open_buffer(preview, Placement::Background);
    editor.show_in_window(editor.split_tree.focused_window_id(), id);

    press(&mut editor, &keys("iX<Esc>"));

    assert_eq!(editor.active_document().buffer().to_string(), "Xpreview");
    let previous = editor.document_manager.get_document(original).unwrap();
    assert_eq!(previous.buffer().to_string(), "original");
}

#[test]
fn test_vsplit_divider_appears() {
    let mut editor = create_editor_sized(10, 40);
    set_content(&mut editor, "hello world\n");
    do_vsplit(&mut editor);
    let screen = render_ascii(&mut editor);
    assert!(
        screen.contains('│'),
        "vsplit divider should be visible\n{screen}"
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
        "divider should be present before and after\nbefore:\n{before}\nafter:\n{after}"
    );
    assert_ne!(
        before_col, after_col,
        "divider column should shift\nbefore:\n{before}\nafter:\n{after}"
    );
}

#[test]
fn test_resize_pane_only_shifts_divider_not_all_content() {
    let mut editor = create_editor_sized(10, 60);
    set_content(&mut editor, "hello world\n");
    do_vsplit(&mut editor);
    do_vsplit(&mut editor);

    let before = render_ascii(&mut editor);
    let before_divs = divider_cols(&before);

    do_resize_pane(&mut editor, -5);

    let after = render_ascii(&mut editor);
    let after_divs = divider_cols(&after);

    assert_eq!(
        before_divs.len(),
        after_divs.len(),
        "divider count unchanged\nbefore:\n{before}\nafter:\n{after}"
    );
    assert_eq!(
        before_divs[0], after_divs[0],
        "left divider fixed\nbefore:{before_divs:?} after:{after_divs:?}"
    );
    assert_ne!(
        before_divs[1], after_divs[1],
        "right divider moved\nbefore:{before_divs:?} after:{after_divs:?}"
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
            "row must be exactly {cols} chars wide"
        );
    }
    let div_col = screen.lines().next().and_then(|l| l.find('│'));
    assert!(
        div_col.is_some_and(|c| c > 0 && c < cols - 1),
        "divider should be inside the screen\n{screen}"
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
fn resize_that_moves_the_cursor_leaves_a_single_soft_cursor() {
    let mut editor = create_editor_sized(24, 40);
    set_content(&mut editor, &format!("{}\nbbbb\n", "a".repeat(60)));
    let second_line = editor
        .active_document()
        .buffer()
        .line_index
        .get_line_start(1);
    editor
        .active_document()
        .set_cursor(second_line, crate::document::ops::CursorCause::User)
        .unwrap();
    editor.update_and_render().unwrap();

    editor.term.size = (24, 80);
    editor.render_system.resize(24, 80);
    editor.update_and_render().unwrap();

    let layer = editor
        .render_system
        .compositor
        .get_layer(crate::layer::LayerPriority::CURSOR)
        .expect("cursor layer exists after a render");
    let cursors: Vec<(usize, usize)> = (0..layer.rows())
        .flat_map(|r| (0..layer.cols()).map(move |c| (r, c)))
        .filter(|&(r, c)| layer.get_cell(r, c).is_some())
        .collect();
    assert_eq!(cursors.len(), 1, "stale cursor cells: {cursors:?}");
}

#[cfg(feature = "plugins")]
#[test]
fn terminal_resize_event_fires_window_resized_with_current_window_size() {
    let mut editor = editor_with_text("hello\n");
    let err = editor.plugin_host.lua_exec(
        r#"
        resized = nil
        rift.on("WindowResized", function(ev)
            local rows, cols = rift.get_window_size()
            resized = { ev.rows, ev.cols, rows, cols }
        end)
        "#,
    );
    assert!(err.is_none(), "{err:?}");

    send_events(&mut editor, [crate::key::InputEvent::Resize(100, 30)]);

    let err = editor.plugin_host.lua_exec(
        r#"
        assert(resized ~= nil, "WindowResized did not fire")
        local got = table.concat(resized, ",")
        assert(got == "30,100,30,100", got)
        "#,
    );
    assert!(err.is_none(), "{err:?}");
    assert_eq!(editor.render_system.compositor.cols(), 100);
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
        crate::command_line::commands::types::SplitSubcommand::File(
            "nonexistent_file_xyz.txt".to_string(),
        ),
    );

    assert_eq!(editor.split_tree.window_count(), 1);
    assert!(!editor.state.notifications.is_empty());
}
