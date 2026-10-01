use super::*;
use crate::test_utils::MockTerminal;

pub(super) fn create_editor() -> Editor<MockTerminal> {
    let term = MockTerminal::new(24, 80);
    Editor::new(term).unwrap()
}

pub(super) fn create_editor_sized(rows: u16, cols: u16) -> Editor<MockTerminal> {
    Editor::new(MockTerminal::new(rows, cols)).unwrap()
}

pub(super) fn render_ascii(editor: &mut Editor<MockTerminal>) -> String {
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

pub(super) fn ring_text(editor: &Editor<MockTerminal>, index: usize) -> Option<String> {
    editor.clipboard_ring.get(index).map(|chars| {
        chars
            .iter()
            .map(crate::character::Character::to_char_lossy)
            .collect()
    })
}

pub(super) fn do_vsplit(editor: &mut Editor<MockTerminal>) {
    editor.do_split_window(
        crate::split::tree::SplitDirection::Vertical,
        crate::command_line::commands::SplitSubcommand::Current,
    );
    editor.update_and_render().unwrap();
}

pub(super) fn do_resize_pane(editor: &mut Editor<MockTerminal>, delta: i32) {
    editor.do_split_window(
        crate::split::tree::SplitDirection::Vertical,
        crate::command_line::commands::SplitSubcommand::Resize(delta),
    );
    editor.update_and_render().unwrap();
}

pub(super) fn set_content(editor: &mut Editor<MockTerminal>, text: &str) {
    let doc = editor.active_document();
    doc.buffer.move_to_start();
    let len = doc.buffer.len();
    for _ in 0..len {
        doc.buffer.delete_forward();
    }
    doc.buffer.insert_str(text).unwrap();
    doc.buffer.move_to_start();
}

pub(super) fn divider_cols(screen: &str) -> Vec<usize> {
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

pub(super) fn split_current(
    editor: &mut Editor<MockTerminal>,
    direction: crate::split::tree::SplitDirection,
) {
    editor.do_split_window(
        direction,
        crate::command_line::commands::SplitSubcommand::Current,
    );
}

pub(super) fn load_text(editor: &mut Editor<MockTerminal>, text: &str) {
    let doc = editor.active_document();
    doc.buffer.move_to_start();
    doc.buffer.insert_str(text).unwrap();
    doc.buffer.move_to_start();
}

pub(super) fn drain_jobs(editor: &mut Editor<MockTerminal>) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match editor
            .job_manager
            .receiver()
            .recv_timeout(Duration::from_millis(50))
        {
            Ok(msg) => {
                let _ = editor.handle_job_message(msg);
            }
            Err(_) => {
                if Instant::now() >= deadline {
                    break;
                }
                if editor.job_manager.receiver().try_recv().is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(feature = "treesitter")]
pub(super) fn drain_jobs_until_idle(editor: &mut Editor<MockTerminal>) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        while let Ok(msg) = editor
            .job_manager
            .receiver()
            .recv_timeout(Duration::from_millis(50))
        {
            let _ = editor.handle_job_message(msg);
        }
        if !editor.job_manager.any_job_thread_alive() || Instant::now() >= deadline {
            break;
        }
    }
}

#[cfg(feature = "treesitter")]
pub(super) fn ensure_syntax_parsed(editor: &mut Editor<MockTerminal>) {
    let has_tree = editor
        .active_document()
        .syntax
        .as_ref()
        .is_some_and(|s| s.tree.is_some());
    if has_tree {
        return;
    }
    let doc_id = editor.active_document().id;
    editor.spawn_syntax_parse_job_immediate(doc_id);
    drain_jobs_until_idle(editor);
}

pub(super) fn feed_keys(editor: &mut Editor<MockTerminal>, seq: &str) {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let keys = crate::key::parse_key_sequence(seq).expect("valid key sequence");
    for key in keys {
        if matches!(editor.current_mode, Mode::Search | Mode::Command) {
            match key {
                Key::Char(c) => {
                    editor.state.append_to_command_line(c);
                    continue;
                }
                Key::Enter => {
                    editor.handle_action(&Action::Editor(EditorAction::Submit));
                    continue;
                }
                _ => {}
            }
        }

        let context = if editor.current_mode.is_visual() {
            KeyContext::Visual
        } else {
            KeyContext::Normal
        };
        if let MatchResult::Exact(action) | MatchResult::Ambiguous(action) =
            editor.keymap.lookup(context, &[key])
        {
            let action = action.clone();
            editor.handle_action(&action);
        }
    }
}

#[cfg(feature = "treesitter")]
pub(super) fn measure_type(editor: &mut Editor<MockTerminal>, label: &str, n: usize) {
    use std::time::Instant;
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        editor.execute_buffer_command(crate::command::Command::InsertChar('x'));
        editor.update_and_render().unwrap();
        times.push(t.elapsed());
    }
    times.sort();
    let avg = times.iter().sum::<std::time::Duration>() / n as u32;
    eprintln!(
        "{label:<16} avg={avg:>9.2?} p50={:>9.2?} p95={:>9.2?} max={:>9.2?}",
        times[n / 2],
        times[n * 95 / 100],
        times[n - 1]
    );
}

pub(super) fn chars_of(s: &str) -> Vec<crate::key::Key> {
    s.chars().map(crate::key::Key::Char).collect()
}

pub(super) fn compositor_ascii<T: crate::term::TerminalBackend>(editor: &mut Editor<T>) -> String {
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
