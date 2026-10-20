//! No-op `Terminal` for when `terminal_emulation` is disabled. `new` always
//! fails, so the rest of the API exists only so callers don't need their own gate.

use super::TerminalEvent;
use std::sync::mpsc;

#[derive(Debug)]
pub struct Terminal {
    pub size: (u16, u16),
    pub name: String,
}

impl Terminal {
    /// # Errors
    /// Always returns `Err`.
    pub fn new(
        _rows: u16,
        _cols: u16,
        _shell_cmd: Option<String>,
    ) -> anyhow::Result<(Self, mpsc::Receiver<TerminalEvent>)> {
        Err(anyhow::anyhow!(
            "terminal emulation is not available in this build"
        ))
    }

    /// # Errors
    /// Never fails; the stub only records the size.
    pub const fn resize(&mut self, rows: u16, cols: u16) -> std::io::Result<()> {
        self.size = (rows, cols);
        Ok(())
    }

    /// # Errors
    /// Never fails; the stub discards the data.
    pub const fn write(&mut self, _data: &[u8]) -> std::io::Result<()> {
        Ok(())
    }

    pub const fn scroll_display(&self, _delta: i32) {}

    pub const fn scroll_to_bottom(&self) {}

    #[must_use]
    pub fn read_screen(&self) -> (String, usize, usize, crate::color::CellColorSpans) {
        (String::new(), 0, 0, crate::color::CellColorSpans::default())
    }
}

#[cfg(test)]
#[path = "terminal_stub_tests.rs"]
mod terminal_stub_tests;
