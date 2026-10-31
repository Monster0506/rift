use super::common::{editor_with_text, keys, press, ring_text};
use crate::action::{Action, EditorAction};
use crate::document::runtime::SaveResult;

#[test]
fn clipboard_preview_is_read_only_and_does_not_save_index_data() {
    let mut editor = editor_with_text("one");
    press(&mut editor, &keys("yy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    let layout = editor.panel_layout.clone().unwrap();
    editor.split_tree.set_focus(layout.preview_win_id);
    editor
        .document_manager
        .switch_to_document(layout.preview_doc_id)
        .unwrap();
    editor.sync_state_with_active_document();

    assert!(editor.active_document().is_read_only());
    assert!(!editor.active_document().is_clipboard_index());
    press(&mut editor, &keys("iX<Esc>xuu<C-r>:w<CR>"));

    assert_eq!(editor.active_document().buffer.to_string(), "one\n");
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("one\n"));
}

#[test]
fn clipboard_enter_allows_multiline_entry_edit_and_marks_save_clean() {
    let mut editor = editor_with_text("one");
    press(&mut editor, &keys("yy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    press(&mut editor, &keys("<CR>iA<CR>B<Esc>:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("A\nBone\n"));
    assert!(!editor.active_document().is_read_only());
    assert!(!editor.active_document().is_dirty());
}

#[test]
fn clipboard_entry_save_preserves_pending_number_move() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    let layout = editor.panel_layout.clone().unwrap();
    press(&mut editor, &keys("0lxi1<Esc><CR>iedited<CR><Esc>:w<CR>"));

    assert_eq!(ring_text(&editor, 0).as_deref(), Some("edited\ntwo\n"));
    let index = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .unwrap();
    assert_eq!(index.buffer.to_string(), "[1]\n[1]");
    assert!(index.is_dirty());

    editor.handle_clipboard_entry_close();
    press(&mut editor, &keys(":w<CR>"));
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("one\n"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("edited\ntwo\n"));
}

#[test]
fn clipboard_return_and_navigation_preserve_dirty_entry() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    let layout = editor.panel_layout.clone().unwrap();
    press(&mut editor, &keys("<CR>iX<Esc>"));
    editor.handle_clipboard_entry_close();
    press(&mut editor, &keys("j<CR>"));

    let preview = editor
        .document_manager
        .get_document(layout.preview_doc_id)
        .unwrap();
    assert_eq!(preview.buffer.to_string(), "Xtwo\n");
    assert!(preview.is_dirty());
    assert_eq!(editor.active_document_id(), layout.dir_doc_id);
}

#[test]
fn clipboard_refresh_preserves_dirty_index() {
    let mut editor = editor_with_text("one\ntwo");
    press(&mut editor, &keys("yyjyy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    press(&mut editor, &keys("0lxi1<Esc>"));
    editor.refresh_clipboard_buffer_if_open();

    assert_eq!(editor.active_document().buffer.to_string(), "[1]\n[1]");
    assert!(editor.active_document().is_dirty());
}

#[test]
fn clipboard_entry_save_rejects_changed_ring_without_marking_clean() {
    let mut editor = editor_with_text("one");
    press(&mut editor, &keys("yy"));
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    press(&mut editor, &keys("<CR>iX<Esc>"));
    editor
        .clipboard_ring
        .push_str("external", crate::wrap::RangeKind::Charwise);

    assert_eq!(editor.apply_clipboard_entry_save(), SaveResult::Failed);
    assert!(editor.active_document().is_dirty());
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("external"));
    assert_eq!(ring_text(&editor, 1).as_deref(), Some("one\n"));
}

#[test]
fn clipboard_new_entry_repeated_save_does_not_duplicate() {
    let mut editor = editor_with_text("");
    editor.handle_action(&Action::Editor(EditorAction::OpenClipboard));
    editor.handle_clipboard_buffer_action("clipboard:new");
    press(&mut editor, &keys("fresh<Esc>:w<CR>:w<CR>"));

    assert_eq!(editor.clipboard_ring.len(), 1);
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("fresh"));
}
