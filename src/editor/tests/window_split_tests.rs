use super::common::*;

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
