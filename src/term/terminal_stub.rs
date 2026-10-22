//! No-op `Terminal` for when `terminal_emulation` is disabled. `new` always
//! fails, so the rest of the API exists only so callers don't need their own gate.

use super::TerminalEvent;
use std::sync::mpsc;

#[derive(Debug)]
pub(crate) struct Terminal {
    pub(crate) size: (u16, u16),
    pub(crate) name: String,
}

impl Terminal {
    /// # Errors
    /// Always returns `Err`.
    pub(crate) fn new(
        _rows: u16,
        _cols: u16,
        _shell_cmd: Option<String>,
    ) -> anyhow::Result<(Self, mpsc::Receiver<TerminalEvent>)> {
        Err(anyhow::anyhow!(
            "terminal emulation is not available in this build"
        ))
    }

    pub(crate) const fn resize(&mut self, rows: u16, cols: u16) {
        self.size = (rows, cols);
    }

    pub(crate) const fn write(&mut self, _data: &[u8]) {}

    pub(crate) const fn scroll_display(&self, _delta: i32) {}

    pub(crate) const fn scroll_to_bottom(&self) {}

    #[must_use]
    pub(crate) fn read_screen(&self) -> (String, usize, usize, crate::color::CellColorSpans) {
        (String::new(), 0, 0, crate::color::CellColorSpans::default())
    }
}

#[cfg(test)]
#[path = "terminal_stub_tests.rs"]
mod terminal_stub_tests;
