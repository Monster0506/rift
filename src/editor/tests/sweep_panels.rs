use super::common::{editor_with_text, keys, press, ring_text};
use crate::action::{Action, EditorAction};

#[test]
fn bug_clipboard_index_edit_wipes_ring() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("A!<Esc>:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("one\n"));
    assert_eq!(editor.clipboard_ring.len(), 1);
    assert_eq!(editor.active_document().buffer.to_string(), "[0]");
}

#[test]
fn clipboard_number_collision_shifts_neighbors() {
    let mut editor = editor_with_text("one\ntwo\nthree");
    press(&mut editor, &keys("yyjyyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("G0lxi0<Esc>:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("one\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("three\n"));
    assert_eq!(ring_text(&editor, 2).as_deref(), Some("two\n"));
    assert_eq!(editor.clipboard_ring.len(), 3);
    assert!(!editor.active_document().is_dirty());
    press(&mut editor, &keys("gg<CR>"));
    assert_eq!(editor.active_document().buffer.to_string(), "one\n");
}

#[test]
fn clipboard_move_down_preserves_entries() {
    let mut editor = editor_with_text("one\ntwo\nthree");
    press(&mut editor, &keys("yyjyyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("0lxi2<Esc>:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("one\n"));
    assert_eq!(ring_text(&editor, 2).as_deref(), Some("three\n"));
}

#[test]
fn clipboard_index_delete_drops_entry() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("dd:w<CR>"));

    assert_eq!(editor.clipboard_ring.len(), 1);
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("one\n"));
    assert!(!editor.active_document().is_dirty());
}

#[test]
fn clipboard_panel_ddp_reorders_rows() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("ddp:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("one\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("two\n"));
}

#[test]
fn clipboard_brackets_cannot_be_edited() {
    let mut editor = editor_with_text("one");
    press(&mut editor, &keys("yy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("0xli<BS>!<Esc>$x"));

    assert_eq!(editor.active_document().buffer.to_string(), "[0]");
    press(&mut editor, &keys(":w<CR>"));
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("one\n"));
    assert!(!editor.active_document().is_dirty());
}

#[test]
fn clipboard_invalid_position_keeps_stack() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("0lxi9<Esc>:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("one\n"));
    assert!(editor.active_document().is_dirty());
}

#[test]
fn clipboard_last_row_delete() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("Gdd:w<CR>"));

    assert_eq!(editor.clipboard_ring.len(), 1);
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("two\n"));
}

#[test]
fn clipboard_delete_undo_keeps_ids() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("Gddu:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("one\n"));
}

#[test]
fn clipboard_copy_keeps_both_clips() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("yyp:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 2).as_deref(), Some("one\n"));
}

#[test]
fn clipboard_visual_put_uses_rows() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("yyVp"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("one\n"));
    assert_eq!(
        editor.apply_clipboard_diff(),
        crate::document::runtime::SaveResult::Saved,
    );
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 2).as_deref(), Some("one\n"));
}

#[test]
fn clipboard_capacity_rejects_loss() {
    let mut editor = editor_with_text("one\ntwo");
    editor.clipboard_ring.set_capacity(2);
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));

    press(&mut editor, &keys("yyp:w<CR>"));

    assert_eq!(editor.clipboard_ring.len(), 2);
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("two\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("one\n"));
    assert!(editor.active_document().is_dirty());
}

#[test]
fn clipboard_malformed_save_atomic() {
    let mut editor = editor_with_text("one");
    press(&mut editor, &keys("yy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    editor.active_document().buffer.move_to_end();
    editor.active_document().buffer.insert_str("!").unwrap();

    assert_eq!(
        editor.apply_clipboard_diff(),
        crate::document::runtime::SaveResult::Failed,
    );
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("one\n"));
    assert_eq!(editor.active_document().buffer.to_string(), "[0]!");
}

#[test]
fn clipboard_stale_save_atomic() {
    let mut editor = editor_with_text("one");
    press(&mut editor, &keys("yy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    editor
        .clipboard_ring
        .push_str("new", crate::wrap::RangeKind::Charwise);

    assert_eq!(
        editor.apply_clipboard_diff(),
        crate::document::runtime::SaveResult::Failed,
    );
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("new"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("one\n"));
}
