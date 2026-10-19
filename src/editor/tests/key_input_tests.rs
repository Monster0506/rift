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
        self.inner.deinit()
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
