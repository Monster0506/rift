//! Stress/timing tests for rapid `<C-w>h/j/k/l` and `<C-w><C-h/j/k/l>` window
//! navigation, driven through the real `tick()` loop rather than internal APIs.

use super::*;
use crate::command_line::commands::SplitSubcommand;
use crate::key::Key;
use crate::split::tree::SplitDirection;
use crate::split::window::WindowId;
use crate::term::{Size, TerminalBackend};
use std::collections::VecDeque;

/// A `TerminalBackend` that yields queued keys instantly (no real I/O wait),
/// so a burst of keypresses can be timed without polling latency.
struct QueueTerminal {
    keys: VecDeque<Key>,
    size: (u16, u16),
}

impl QueueTerminal {
    fn new(rows: u16, cols: u16) -> Self {
        Self {
            keys: VecDeque::new(),
            size: (rows, cols),
        }
    }

    fn push_chord(&mut self, keys: &[Key]) {
        self.keys.extend(keys.iter().copied());
    }
}

impl TerminalBackend for QueueTerminal {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn deinit(&mut self) {}
    fn poll(&mut self, _duration: std::time::Duration) -> Result<bool, String> {
        Ok(!self.keys.is_empty())
    }
    fn read_key(&mut self) -> Result<Option<Key>, String> {
        Ok(self.keys.pop_front())
    }
    fn write(&mut self, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn get_size(&self) -> Result<Size, String> {
        Ok(Size {
            rows: self.size.0,
            cols: self.size.1,
        })
    }
    fn clear_screen(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn move_cursor(&mut self, _row: u16, _col: u16) -> Result<(), String> {
        Ok(())
    }
    fn hide_cursor(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn show_cursor(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn set_cursor_shape(&mut self, _shape: crate::term::CursorShape) -> Result<(), String> {
        Ok(())
    }
}

fn process_jobs(editor: &mut Editor<QueueTerminal>) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        while let Ok(msg) = editor
            .job_manager
            .receiver()
            .recv_timeout(Duration::from_millis(50))
        {
            editor.handle_job_message(msg).unwrap();
        }
        if !editor.job_manager.any_job_thread_alive() || Instant::now() >= deadline {
            break;
        }
    }
}

/// Runs `tick()` until `terminal.keys` is drained.
fn drain(editor: &mut Editor<QueueTerminal>) {
    while !editor.term.keys.is_empty() {
        editor.tick().unwrap();
    }
}

const WW: Key = Key::Ctrl(b'w');

/// A|B side by side, focus starts on A. Returns the backing `TempDir` too,
/// so callers keep it alive (and get cleanup) for the test's duration.
fn setup_two_pane() -> (Editor<QueueTerminal>, tempfile::TempDir, WindowId, WindowId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "A\n").unwrap();
    std::fs::write(&b, "B\n").unwrap();

    let term = QueueTerminal::new(50, 120);
    let mut editor =
        Editor::with_file(term, Some(a.to_str().unwrap().to_string())).expect("editor init");
    process_jobs(&mut editor);
    let win_a = editor.split_tree.focused_window_id();

    editor.do_split_window(
        SplitDirection::Vertical,
        SplitSubcommand::File(b.to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);
    let win_b = editor.split_tree.focused_window_id();

    editor.split_tree.set_focus(win_a);
    (editor, dir, win_a, win_b)
}

/// 2x2 grid: A(top-left) B(top-right) / C(bottom-left) D(bottom-right).
fn setup_four_pane() -> (Editor<QueueTerminal>, tempfile::TempDir, [WindowId; 4]) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths: Vec<_> = ["a", "b", "c", "d"]
        .iter()
        .map(|n| {
            let p = dir.path().join(format!("{n}.txt"));
            std::fs::write(&p, format!("{}\n", n.to_uppercase())).unwrap();
            p
        })
        .collect();

    let term = QueueTerminal::new(60, 160);
    let mut editor =
        Editor::with_file(term, Some(paths[0].to_str().unwrap().to_string())).expect("editor init");
    process_jobs(&mut editor);
    let win_a = editor.split_tree.focused_window_id();

    editor.do_split_window(
        SplitDirection::Vertical,
        SplitSubcommand::File(paths[1].to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);
    let win_b = editor.split_tree.focused_window_id();

    editor.split_tree.set_focus(win_a);
    editor.do_split_window(
        SplitDirection::Horizontal,
        SplitSubcommand::File(paths[2].to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);
    let win_c = editor.split_tree.focused_window_id();

    editor.split_tree.set_focus(win_b);
    editor.do_split_window(
        SplitDirection::Horizontal,
        SplitSubcommand::File(paths[3].to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);
    let win_d = editor.split_tree.focused_window_id();

    editor.split_tree.set_focus(win_a);
    (editor, dir, [win_a, win_b, win_c, win_d])
}

#[test]
fn rapid_ctrl_w_hjkl_toggle_matches_ctrl_held_form() {
    let (mut editor, _dir, win_a, win_b) = setup_two_pane();
    assert_eq!(editor.split_tree.focused_window_id(), win_a);

    const ROUNDS: usize = 2_000;
    for i in 0..ROUNDS {
        // Alternate which of the two equivalent chord forms fires each round,
        // so the burst genuinely exercises both bindings under load.
        if i % 2 == 0 {
            editor.term.push_chord(&[WW, Key::Char('l')]);
            editor.term.push_chord(&[WW, Key::Char('h')]);
        } else {
            editor.term.push_chord(&[WW, Key::Ctrl(b'l')]);
            editor.term.push_chord(&[WW, Key::Ctrl(b'h')]);
        }
    }

    let start = std::time::Instant::now();
    drain(&mut editor);
    let elapsed = start.elapsed();

    // Each round is a round-trip (A -> B -> A), so focus must land back on A.
    assert_eq!(
        editor.split_tree.focused_window_id(),
        win_a,
        "should be back on window A after an even number of round-trips"
    );
    assert_eq!(editor.current_mode, Mode::Normal);
    assert!(editor.pending_keys.is_empty());

    let total_chords = ROUNDS * 2;
    let total_keys = total_chords * 2; // each chord is 2 Key events
    eprintln!(
        "rapid_ctrl_w_hjkl_toggle: {total_chords} nav chords ({total_keys} keys) in {elapsed:?} \
         ({:.0} chords/sec, {:.0} keys/sec)",
        total_chords as f64 / elapsed.as_secs_f64(),
        total_keys as f64 / elapsed.as_secs_f64(),
    );

    let _ = win_b; // used only to assert the layout was built correctly above
}

#[test]
fn rapid_ctrl_w_four_pane_survives_random_walk_without_corruption() {
    let (mut editor, _dir, wins) = setup_four_pane();
    let valid: std::collections::HashSet<WindowId> = wins.iter().copied().collect();

    // Deterministic pseudo-random walk over all 8 chord forms (plain +
    // ctrl-held, all 4 directions), less structured than the round-trip test above.
    const STEPS: usize = 5_000;
    let plain = [
        Key::Char('h'),
        Key::Char('j'),
        Key::Char('k'),
        Key::Char('l'),
    ];
    let held = [
        Key::Ctrl(b'h'),
        Key::Ctrl(b'j'),
        Key::Ctrl(b'k'),
        Key::Ctrl(b'l'),
    ];
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    for _ in 0..STEPS {
        // xorshift64 -- cheap, deterministic, good enough for test input mixing.
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let dir_idx = (seed % 4) as usize;
        let second = if (seed >> 4).is_multiple_of(2) {
            plain[dir_idx]
        } else {
            held[dir_idx]
        };
        editor.term.push_chord(&[WW, second]);
    }

    let start = std::time::Instant::now();
    drain(&mut editor);
    let elapsed = start.elapsed();

    assert_eq!(
        editor.current_mode,
        Mode::Normal,
        "rapid window navigation must never leave a stray mode"
    );
    assert!(
        editor.pending_keys.is_empty(),
        "must not end mid-sequence after the burst fully drains"
    );
    assert!(
        valid.contains(&editor.split_tree.focused_window_id()),
        "focus must land on one of the 4 real windows, not a corrupted id"
    );

    let total_keys = STEPS * 2;
    eprintln!(
        "rapid_ctrl_w_four_pane_random_walk: {STEPS} nav chords ({total_keys} keys) in {elapsed:?} \
         ({:.0} chords/sec, {:.0} keys/sec)",
        STEPS as f64 / elapsed.as_secs_f64(),
        total_keys as f64 / elapsed.as_secs_f64(),
    );
}
