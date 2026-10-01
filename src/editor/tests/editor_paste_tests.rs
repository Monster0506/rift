use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

struct ScriptedTerminal {
    inner: MockTerminal,
    queue: std::collections::VecDeque<crate::key::Key>,
}

impl ScriptedTerminal {
    fn new(rows: u16, cols: u16, keys: Vec<crate::key::Key>) -> Self {
        Self {
            inner: MockTerminal::new(rows, cols),
            queue: keys.into_iter().collect(),
        }
    }
}

impl crate::term::TerminalBackend for ScriptedTerminal {
    fn init(&mut self) -> Result<(), String> {
        self.inner.init()
    }
    fn deinit(&mut self) {
        self.inner.deinit()
    }
    fn poll(&mut self, _duration: std::time::Duration) -> Result<bool, String> {
        Ok(!self.queue.is_empty())
    }
    fn read_key(&mut self) -> Result<Option<crate::key::Key>, String> {
        Ok(self.queue.pop_front())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> Result<(), String> {
        self.inner.flush()
    }
    fn get_size(&self) -> Result<crate::term::Size, String> {
        self.inner.get_size()
    }
    fn clear_screen(&mut self) -> Result<(), String> {
        self.inner.clear_screen()
    }
    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String> {
        self.inner.move_cursor(row, col)
    }
    fn hide_cursor(&mut self) -> Result<(), String> {
        self.inner.hide_cursor()
    }
    fn show_cursor(&mut self) -> Result<(), String> {
        self.inner.show_cursor()
    }
    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        self.inner.clear_to_end_of_line()
    }
    fn set_cursor_shape(&mut self, shape: crate::term::CursorShape) -> Result<(), String> {
        self.inner.set_cursor_shape(shape)
    }
}

#[test]
fn pasting_a_shell_command_into_insert_mode_inserts_every_character() {
    let mut keys = vec![crate::key::Key::Char('i')];
    keys.extend(chars_of(" 1 npm install -g @openai/codex"));
    let mut editor = Editor::new(ScriptedTerminal::new(24, 80, keys)).unwrap();

    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
    }

    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(
        editor.active_document().buffer.to_string(),
        " 1 npm install -g @openai/codex"
    );
}

#[test]
fn pasting_a_shell_command_into_normal_mode_does_not_panic_or_hang() {
    let keys = chars_of(" 1 npm install -g @openai/codex");
    let mut editor = Editor::new(ScriptedTerminal::new(24, 80, keys)).unwrap();

    let mut ticks = 0;
    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
        ticks += 1;
        assert!(
            ticks < 10_000,
            "tick() loop must drain the scripted queue, not spin"
        );
    }

    let content = editor.active_document().buffer.to_string();
    eprintln!(
        "after Normal-mode paste: mode={:?} pending_keys={:?} buffer={:?}",
        editor.current_mode, editor.pending_keys, content
    );
}

#[test]
fn bracketed_paste_in_insert_mode_renders_the_full_text_without_a_second_render() {
    use crate::action::{Action, EditorAction};
    let mut editor = create_editor();
    editor.update_and_render().unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    editor
        .handle_paste(" 4 CS Classes to Visit".to_string())
        .unwrap();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        " 4 CS Classes to Visit"
    );
    let screen = compositor_ascii(&mut editor);
    assert!(
        screen.contains(" 4 CS Classes to Visit"),
        "screen must show the full pasted text after a single render:\n{screen}"
    );
}

#[test]
fn bracketed_paste_near_soft_wrap_boundary_renders_the_full_text() {
    use crate::action::{Action, EditorAction};
    let mut editor = create_editor_sized(24, 20);
    editor.update_and_render().unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    for ch in "0123456789012345".chars() {
        editor.execute_buffer_command(crate::command::Command::InsertChar(ch));
        editor.update_and_render().unwrap();
    }

    editor
        .handle_paste(" 4 CS Classes to Visit".to_string())
        .unwrap();

    let expected = "0123456789012345 4 CS Classes to Visit";
    assert_eq!(editor.active_document().buffer.to_string(), expected);

    let screen = compositor_ascii(&mut editor);
    let joined: String = screen.chars().filter(|c| *c != '\n').collect();
    assert!(
        joined.contains("4 CS Classes to Visit")
            || joined.replace(' ', "").contains("4CSClassestoVisit"),
        "screen must show the pasted tail even when the line wraps:\n{screen}"
    );
}

#[cfg(feature = "terminal_emulation")]
#[derive(Clone, Default)]
struct SharedBuf(std::sync::Arc<parking_lot::Mutex<Vec<u8>>>);

#[cfg(feature = "terminal_emulation")]
impl std::io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "terminal_emulation")]
#[derive(Clone)]
struct NoopListener;

#[cfg(feature = "terminal_emulation")]
impl alacritty_terminal::event::EventListener for NoopListener {
    fn send_event(&self, _event: alacritty_terminal::event::Event) {}
}

#[cfg(feature = "terminal_emulation")]
#[derive(Debug, Clone, Copy)]
struct VteDims {
    rows: usize,
    cols: usize,
}

#[cfg(feature = "terminal_emulation")]
impl alacritty_terminal::grid::Dimensions for VteDims {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

#[cfg(feature = "terminal_emulation")]
#[test]
fn bracketed_paste_reproduced_through_a_real_vte_parser() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::replay::ReplayBackend;

    let sink = SharedBuf::default();
    let backend = ReplayBackend::new(sink.clone(), 24, 80);
    let mut editor = Editor::new(backend).unwrap();

    editor.update_and_render().unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    for ch in "Top of the list: ".chars() {
        editor.execute_buffer_command(crate::command::Command::InsertChar(ch));
        editor.update_and_render().unwrap();
    }
    sink.0.lock().clear();

    editor
        .term
        .push_keys([Key::Paste(" 4 CS Classes to Visit".to_string())]);
    editor.tick().unwrap();

    let bytes = sink.0.lock().clone();
    assert!(!bytes.is_empty(), "paste must produce terminal output");

    let listener = NoopListener;
    let dims = VteDims { rows: 24, cols: 80 };
    let term = alacritty_terminal::term::Term::new(
        alacritty_terminal::term::Config::default(),
        &dims,
        listener,
    );
    let term = std::sync::Arc::new(alacritty_terminal::sync::FairMutex::new(term));
    let mut parser = alacritty_terminal::vte::ansi::Processor::<
        alacritty_terminal::vte::ansi::StdSyncHandler,
    >::new();
    parser.advance(&mut *term.lock(), &bytes);

    let expected = "Top of the list:  4 CS Classes to Visit";
    assert_eq!(editor.active_document().buffer.to_string(), expected);

    use alacritty_terminal::grid::Dimensions as _;
    let t = term.lock();
    let grid = t.grid();
    let line0: String = (0..grid.columns())
        .map(|c| grid[alacritty_terminal::index::Line(0)][alacritty_terminal::index::Column(c)].c)
        .collect();
    assert!(
        line0.contains("4 CS Classes to Visit"),
        "a real terminal parsing rift's own output bytes must show the full pasted text:\n{line0:?}"
    );
}

struct PressReleaseTerminal {
    inner: MockTerminal,
    raw: std::collections::VecDeque<Option<crate::key::Key>>,
}

impl PressReleaseTerminal {
    fn new(rows: u16, cols: u16, keys: Vec<crate::key::Key>) -> Self {
        let mut raw = std::collections::VecDeque::new();
        for key in keys {
            raw.push_back(Some(key));
            raw.push_back(None);
        }
        Self {
            inner: MockTerminal::new(rows, cols),
            raw,
        }
    }
}

impl crate::term::TerminalBackend for PressReleaseTerminal {
    fn init(&mut self) -> Result<(), String> {
        self.inner.init()
    }
    fn deinit(&mut self) {
        self.inner.deinit()
    }
    fn poll(&mut self, _duration: std::time::Duration) -> Result<bool, String> {
        Ok(!self.raw.is_empty())
    }
    fn read_key(&mut self) -> Result<Option<crate::key::Key>, String> {
        Ok(self.raw.pop_front().flatten())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> Result<(), String> {
        self.inner.flush()
    }
    fn get_size(&self) -> Result<crate::term::Size, String> {
        self.inner.get_size()
    }
    fn clear_screen(&mut self) -> Result<(), String> {
        self.inner.clear_screen()
    }
    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String> {
        self.inner.move_cursor(row, col)
    }
    fn hide_cursor(&mut self) -> Result<(), String> {
        self.inner.hide_cursor()
    }
    fn show_cursor(&mut self) -> Result<(), String> {
        self.inner.show_cursor()
    }
    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        self.inner.clear_to_end_of_line()
    }
    fn set_cursor_shape(&mut self, shape: crate::term::CursorShape) -> Result<(), String> {
        self.inner.set_cursor_shape(shape)
    }
}

#[test]
fn paste_burst_with_trailing_release_events_still_renders_without_a_further_key() {
    use crate::action::{Action, EditorAction};

    let pasted = " 4 CS Classes to Visit";
    let keys: Vec<crate::key::Key> = pasted.chars().map(crate::key::Key::Char).collect();
    let mut editor = Editor::new(PressReleaseTerminal::new(24, 80, keys)).unwrap();
    editor.update_and_render().unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    let mut ticks = 0;
    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
        ticks += 1;
        assert!(ticks < 10_000, "tick loop must drain the queue, not spin");
    }

    assert_eq!(editor.active_document().buffer.to_string(), pasted);

    let screen = compositor_ascii(&mut editor);
    let joined: String = screen.chars().filter(|c| *c != '\n').collect();
    assert!(
        joined.contains(pasted) || joined.replace(' ', "").contains("4CSClassestoVisit"),
        "screen must show the pasted text once input drains, with no further keypress:\n{screen}"
    );
}

#[test]
fn single_space_keypress_with_release_moves_the_rendered_cursor() {
    use crate::action::{Action, EditorAction};
    let mut editor = Editor::new(PressReleaseTerminal::new(24, 80, vec![])).unwrap();
    editor.update_and_render().unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    editor.execute_buffer_command(crate::command::Command::InsertChar('a'));
    editor.update_and_render().unwrap();
    let before = editor.term.inner.cursor_moves.last().copied();

    editor.term.raw.push_back(Some(crate::key::Key::Char(' ')));
    editor.term.raw.push_back(None);
    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
    }

    assert_eq!(editor.active_document().buffer.to_string(), "a ");
    let after = editor.term.inner.cursor_moves.last().copied();
    assert_ne!(
        after, before,
        "the rendered cursor must advance past the space without a further keypress"
    );
}

#[test]
fn single_up_keypress_on_soft_wrap_with_release_moves_the_rendered_cursor() {
    use crate::action::{Action, EditorAction};
    let mut editor = Editor::new(PressReleaseTerminal::new(24, 20, vec![])).unwrap();
    editor.state.settings.soft_wrap = true;
    editor.execute_buffer_command(crate::command::Command::InsertChar('x'));
    for _ in 0..30 {
        editor.execute_buffer_command(crate::command::Command::InsertChar('a'));
    }
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.update_and_render().unwrap();
    let before = editor.term.inner.cursor_moves.last().copied();

    editor.term.raw.push_back(Some(crate::key::Key::Char('k')));
    editor.term.raw.push_back(None);
    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
    }

    let after = editor.term.inner.cursor_moves.last().copied();
    assert_ne!(
        after, before,
        "the rendered cursor must move up a soft-wrapped row without a further keypress"
    );
}
