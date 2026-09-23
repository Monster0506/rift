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
    pub fn new(
        _rows: u16,
        _cols: u16,
        _shell_cmd: Option<String>,
    ) -> anyhow::Result<(Self, mpsc::Receiver<TerminalEvent>)> {
        Err(anyhow::anyhow!(
            "terminal emulation is not available in this build"
        ))
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> std::io::Result<()> {
        self.size = (rows, cols);
        Ok(())
    }

    pub fn write(&mut self, _data: &[u8]) -> std::io::Result<()> {
        Ok(())
    }

    pub fn scroll_display(&self, _delta: i32) {}

    pub fn scroll_to_bottom(&self) {}

    pub fn read_screen(&self) -> (String, usize, usize, crate::color::CellColorSpans) {
        (String::new(), 0, 0, Default::default())
    }
}

#[cfg(test)]
#[path = "terminal_stub_tests.rs"]
mod terminal_stub_tests;
