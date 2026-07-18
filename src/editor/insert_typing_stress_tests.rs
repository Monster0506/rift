//! Throughput check for rapid Insert-mode typing (unbuffered terminal paste
//! looks identical to this) against a realistic multi-thousand-line buffer.

use super::*;
use crate::key::Key;
use crate::term::{Size, TerminalBackend};
use std::collections::VecDeque;

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
    let deadline = Instant::now() + Duration::from_secs(10);
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

fn drain(editor: &mut Editor<QueueTerminal>) {
    while !editor.term.keys.is_empty() {
        editor.tick().unwrap();
    }
}

/// A ~10k-line, code-shaped buffer -- matching the shape `render_bench`
/// uses for its full-frame render measurement, for a comparable baseline.
/// `.txt`, deliberately: `.rs` would pull in treesitter and LSP tracking
/// based on whatever's in this machine's real plugin config, not a clean number.
fn realistic_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("big.txt");
    let mut content = String::new();
    for i in 0..10_000 {
        content.push_str(&format!(
            "fn function_{i}() {{ println!(\"Hello World {i}\"); }}\n"
        ));
    }
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

#[test]
fn rapid_insert_mode_typing_keeps_up_on_a_large_file() {
    let (_dir, path) = realistic_file();
    let term = QueueTerminal::new(50, 120);
    let mut editor =
        Editor::with_file(term, Some(path.to_str().unwrap().to_string())).expect("editor init");
    process_jobs(&mut editor);

    let start_len = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .len();

    // 'i' enters Insert mode; the rest is a burst of plain letters, exactly
    // how a real fast typist's keystrokes arrive through the terminal.
    editor.term.keys.push_back(Key::Char('i'));
    const CHARS: usize = 5_000;
    let alphabet = "abcdefghijklmnopqrstuvwxyz";
    for i in 0..CHARS {
        let ch = alphabet.as_bytes()[i % alphabet.len()] as char;
        editor.term.keys.push_back(Key::Char(ch));
    }

    let start = std::time::Instant::now();
    drain(&mut editor);
    let elapsed = start.elapsed();

    #[cfg(feature = "perf_instrumentation")]
    {
        let mut stats = crate::perf::span_stats();
        stats.sort_by(|a, b| {
            (b.1.avg_ms * b.1.count as f64).total_cmp(&(a.1.avg_ms * a.1.count as f64))
        });
        for (name, s) in stats {
            eprintln!(
                "  {name}: count={} avg_ms={:.4} total_ms={:.1}",
                s.count,
                s.avg_ms,
                s.avg_ms * s.count as f64
            );
        }
    }

    assert_eq!(editor.current_mode, Mode::Insert);
    let end_len = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .len();
    assert_eq!(
        end_len,
        start_len + CHARS,
        "every typed character must land in the buffer, none dropped"
    );

    eprintln!(
        "rapid_insert_mode_typing: {CHARS} chars typed into a 10k-line file in {elapsed:?} \
         ({:.0} chars/sec)",
        CHARS as f64 / elapsed.as_secs_f64(),
    );
}
