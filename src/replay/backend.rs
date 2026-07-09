//! A `TerminalBackend` that serves keys from a script queue and encodes
//! output through the real `CrosstermBackend` path, without a live TTY.

use crate::key::Key;
use crate::term::crossterm::CrosstermBackend;
use crate::term::{CursorShape, Size, TerminalBackend};
use std::collections::VecDeque;
use std::io::Write;

pub struct ReplayBackend<W: Write> {
    output: CrosstermBackend<W>,
    size: Size,
    pending: VecDeque<Key>,
}

impl<W: Write> ReplayBackend<W> {
    pub fn new(writer: W, rows: u16, cols: u16) -> Self {
        Self {
            output: CrosstermBackend::with_writer(writer),
            size: Size { rows, cols },
            pending: VecDeque::new(),
        }
    }

    /// Queue keys for future `read_key()` calls, oldest first.
    pub fn push_keys(&mut self, keys: impl IntoIterator<Item = Key>) {
        self.pending.extend(keys);
    }
}

impl<W: Write> TerminalBackend for ReplayBackend<W> {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn deinit(&mut self) {}

    fn poll(&mut self, _duration: std::time::Duration) -> Result<bool, String> {
        Ok(!self.pending.is_empty())
    }

    fn read_key(&mut self) -> Result<Option<Key>, String> {
        Ok(self.pending.pop_front())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.output.write(bytes)
    }

    fn flush(&mut self) -> Result<(), String> {
        self.output.flush()
    }

    fn get_size(&self) -> Result<Size, String> {
        Ok(self.size)
    }

    fn clear_screen(&mut self) -> Result<(), String> {
        self.output.clear_screen()
    }

    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String> {
        self.output.move_cursor(row, col)
    }

    fn hide_cursor(&mut self) -> Result<(), String> {
        self.output.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), String> {
        self.output.show_cursor()
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        self.output.clear_to_end_of_line()
    }

    fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), String> {
        self.output.set_cursor_shape(shape)
    }
}
