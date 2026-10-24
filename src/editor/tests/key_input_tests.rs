//! Keys with modifiers, driven through the real `tick()` loop.

use super::*;
use crate::action::{Action, EditorAction};
use crate::key::{Key, Mods};
use crate::test_utils::MockTerminal;
use std::collections::VecDeque;

struct KeyQueue {
    inner: MockTerminal,
    queue: VecDeque<Key>,
}

impl crate::term::TerminalBackend for KeyQueue {
    fn init(&mut self) -> Result<(), crate::error::RiftError> {
        self.inner.init()
    }
    fn deinit(&mut self) {
        self.inner.deinit();
    }
    fn poll(&mut self, _duration: std::time::Duration) -> Result<bool, crate::error::RiftError> {
        Ok(!self.queue.is_empty())
    }
    fn read_event(&mut self) -> Result<Option<crate::key::InputEvent>, crate::error::RiftError> {
        Ok(self.queue.pop_front().map(crate::key::InputEvent::Key))
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), crate::error::RiftError> {
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> Result<(), crate::error::RiftError> {
        self.inner.flush()
    }
    fn get_size(&self) -> Result<crate::term::Size, crate::error::RiftError> {
        self.inner.get_size()
    }
    fn clear_screen(&mut self) -> Result<(), crate::error::RiftError> {
        self.inner.clear_screen()
    }
    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), crate::error::RiftError> {
        self.inner.move_cursor(row, col)
    }
    fn hide_cursor(&mut self) -> Result<(), crate::error::RiftError> {
        self.inner.hide_cursor()
    }
    fn show_cursor(&mut self) -> Result<(), crate::error::RiftError> {
        self.inner.show_cursor()
    }
    fn clear_to_end_of_line(&mut self) -> Result<(), crate::error::RiftError> {
        self.inner.clear_to_end_of_line()
    }
    fn set_cursor_shape(
        &mut self,
        shape: crate::term::CursorShape,
    ) -> Result<(), crate::error::RiftError> {
        self.inner.set_cursor_shape(shape)
    }
}

fn editor_with_text(text: &str) -> Editor<KeyQueue> {
    let term = KeyQueue {
        inner: MockTerminal::new(24, 80),
        queue: VecDeque::new(),
    };
    let mut editor = Editor::new(term).unwrap();
    let doc = editor.active_document();
    doc.buffer.insert_str(text).unwrap();
    doc.buffer.move_to_start();
    editor
}

fn press(editor: &mut Editor<KeyQueue>, keys: &[Key]) {
    editor.term.queue.extend(keys.iter().copied());
    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
    }
}

fn buffer_text(editor: &mut Editor<KeyQueue>) -> String {
    editor.active_document().buffer.to_string()
}

#[test]
fn shift_space_in_insert_mode_types_a_space() {
    let mut editor = editor_with_text("");
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    press(
        &mut editor,
        &[Key::char('a'), Key::SHIFT_SPACE, Key::char('b')],
    );

    assert_eq!(buffer_text(&mut editor), "a b");
}

#[test]
fn shift_space_in_command_mode_types_a_space() {
    let mut editor = editor_with_text("");
    editor.handle_action(&Action::Editor(EditorAction::EnterCommandMode));

    press(
        &mut editor,
        &[Key::char('e'), Key::SHIFT_SPACE, Key::char('x')],
    );

    assert_eq!(editor.state.command_line, "e x");
}

#[test]
fn shift_space_in_search_mode_types_a_space() {
    let mut editor = editor_with_text("");
    editor.handle_action(&Action::Editor(EditorAction::EnterSearchMode));

    press(
        &mut editor,
        &[Key::char('a'), Key::SHIFT_SPACE, Key::char('b')],
    );

    assert_eq!(editor.state.command_line, "a b");
}

#[test]
fn shift_space_in_replace_mode_replaces_with_a_space() {
    let mut editor = editor_with_text("abc");
    editor.handle_action(&Action::Editor(EditorAction::EnterReplaceMode));

    press(&mut editor, &[Key::SHIFT_SPACE]);

    assert_eq!(buffer_text(&mut editor), " bc");
}

#[test]
fn replace_char_accepts_shift_space() {
    let mut editor = editor_with_text("abc");

    press(&mut editor, &[Key::char('r'), Key::SHIFT_SPACE]);

    assert_eq!(buffer_text(&mut editor), " bc");
}

#[test]
fn find_char_accepts_shift_space() {
    let mut editor = editor_with_text("ab cd");

    press(&mut editor, &[Key::char('f'), Key::SHIFT_SPACE]);

    assert_eq!(editor.active_document().buffer.cursor(), 2);
}

#[test]
fn shift_space_in_visual_mode_still_shrinks_the_region() {
    let mut editor = editor_with_text("say \"hello world\" now");
    let pos = buffer_text(&mut editor).find("hello").unwrap();
    editor.active_document().buffer.set_cursor(pos).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    let start = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );

    press(&mut editor, &[Key::SPACE]);
    let grown = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );
    assert_ne!(grown, start, "space must expand the region");

    press(&mut editor, &[Key::SHIFT_SPACE]);
    let shrunk = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );
    assert_eq!(shrunk, start, "shift+space must shrink it back");
}

#[test]
fn ctrl_space_and_alt_chars_are_not_typed_in_insert_mode() {
    let mut editor = editor_with_text("");
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    press(
        &mut editor,
        &[Key::ctrl(' '), Key::alt('x'), Key::char('a')],
    );

    assert_eq!(buffer_text(&mut editor), "a");
}

#[test]
fn function_and_insert_keys_do_nothing_in_text_modes() {
    let mut editor = editor_with_text("");
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    press(
        &mut editor,
        &[
            Key::f(5),
            Key::INSERT,
            Key::f(12).with(Mods::SHIFT),
            Key::char('a'),
        ],
    );

    assert_eq!(buffer_text(&mut editor), "a");
}

#[test]
fn replace_char_ignores_modified_keys() {
    let mut editor = editor_with_text("abc");

    press(&mut editor, &[Key::char('r'), Key::ctrl('x')]);

    assert_eq!(buffer_text(&mut editor), "abc");
}

#[test]
fn shift_arrow_moves_like_the_arrow() {
    let mut editor = editor_with_text("abc");
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    press(&mut editor, &[Key::RIGHT.with(Mods::SHIFT)]);

    assert_eq!(editor.active_document().buffer.cursor(), 1);
}

fn keys(seq: &str) -> Vec<Key> {
    crate::key::notation::parse_key_sequence(seq).unwrap()
}

#[test]
fn dgg_deletes_to_start_of_file() {
    let mut editor = editor_with_text("one\ntwo\nthree");

    press(&mut editor, &keys("jdgg"));

    assert_eq!(buffer_text(&mut editor), "three");
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn cgg_changes_to_start_of_file() {
    let mut editor = editor_with_text("one\ntwo\nthree");

    press(&mut editor, &keys("jcgg"));

    assert_eq!(editor.current_mode, Mode::Insert);
    assert!(buffer_text(&mut editor).ends_with("three"));
    assert!(!buffer_text(&mut editor).contains("two"));
}

#[test]
fn ygg_yanks_to_start_of_file() {
    let mut editor = editor_with_text("one\ntwo\nthree");

    press(&mut editor, &keys("jyggGp"));

    assert_eq!(buffer_text(&mut editor), "one\ntwo\nthree\none\ntwo");
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn sgg_surrounds_current_line() {
    let mut editor = editor_with_text("one\ntwo\nthree");

    press(&mut editor, &keys("jsgg\""));

    assert_eq!(buffer_text(&mut editor), "one\n\"two\"\nthree");
}

#[test]
fn dg_waits_for_second_g() {
    let mut editor = editor_with_text("one\ntwo\nthree");

    press(&mut editor, &keys("jdg"));

    assert_eq!(editor.current_mode, Mode::OperatorPending);
}

#[test]
fn bug_replace_mode_backspace_restores_overwritten() {
    let mut editor = editor_with_text("abcdef");

    press(&mut editor, &keys("Rxy<BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "xbcdef");
}

#[test]
fn bug_replace_mode_backspace_before_typing_keeps_text() {
    let mut editor = editor_with_text("abcdef");

    press(&mut editor, &keys("lllR<BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "abcdef");
}

#[test]
fn bug_replace_mode_backspace_removes_appended() {
    let mut editor = editor_with_text("ab");

    press(&mut editor, &keys("lRxyz<BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "axy");
}

#[test]
fn bug_replace_mode_backspace_stack_resets_per_session() {
    let mut editor = editor_with_text("abc");

    press(&mut editor, &keys("Rx<Esc>lR<BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "xbc");
}

#[test]
fn bug_replace_mode_dot_repeat_replays_session() {
    let mut editor = editor_with_text("abcdefgh");

    press(&mut editor, &keys("Rxy<Esc>0ll."));

    assert_eq!(buffer_text(&mut editor), "xyxyefgh");
}

#[test]
fn bug_replace_mode_dot_repeat_skips_backspaced_text() {
    let mut editor = editor_with_text("abcdef");

    press(&mut editor, &keys("Rxy<BS><Esc>0ll."));

    assert_eq!(buffer_text(&mut editor), "xbxdef");
}

#[test]
fn bug_replace_mode_paste_overwrites() {
    let mut editor = editor_with_text("abcdef");
    press(&mut editor, &keys("R"));

    editor.handle_paste("xy").unwrap();

    assert_eq!(buffer_text(&mut editor), "xycdef");
}

#[test]
fn bug_replace_mode_paste_appends_past_line_end() {
    let mut editor = editor_with_text("ab\ncd");
    press(&mut editor, &keys("lR"));

    editor.handle_paste("xyz").unwrap();

    assert_eq!(buffer_text(&mut editor), "axyz\ncd");
}

#[test]
fn bug_replace_mode_backspace_restores_after_paste() {
    let mut editor = editor_with_text("abcdef");
    press(&mut editor, &keys("R"));
    editor.handle_paste("xy").unwrap();

    press(&mut editor, &keys("<BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "xbcdef");
}

#[test]
fn bug_replace_mode_tab_overwrites_one_char() {
    let mut editor = editor_with_text("abc");
    editor.active_document().options.expand_tabs = false;

    press(&mut editor, &keys("R<Tab><Esc>"));

    assert_eq!(buffer_text(&mut editor), "\tbc");
}

#[test]
fn bug_replace_mode_expanded_tab_overwrites_spaces() {
    let mut editor = editor_with_text("abcdefgh");
    editor.active_document().options.expand_tabs = true;
    editor.active_document().options.tab_width = 4;

    press(&mut editor, &keys("R<Tab><BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "   defgh");
}

#[test]
fn bug_replace_mode_backspace_undoes_line_break() {
    let mut editor = editor_with_text("abcd");

    press(&mut editor, &keys("Rx<CR><BS><BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "abcd");
}

#[test]
fn bug_replace_mode_ctrl_w_restores_typed_word() {
    let mut editor = editor_with_text("abcdef");

    press(&mut editor, &[Key::char('R')]);
    press(&mut editor, &keys("xyz"));
    press(&mut editor, &[Key::ctrl('w'), Key::ESC]);

    assert_eq!(buffer_text(&mut editor), "abcdef");
}

#[test]
fn bug_replace_mode_backspace_after_cursor_move_only_moves() {
    let mut editor = editor_with_text("abcdef");

    press(&mut editor, &keys("Rxy<Left><BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "xycdef");
}

#[test]
fn bug_replace_mode_backspace_after_move_round_trip_only_moves() {
    let mut editor = editor_with_text("abcdef");

    press(&mut editor, &keys("Rx<Left><Right><BS><Esc>"));

    assert_eq!(buffer_text(&mut editor), "xbcdef");
}
