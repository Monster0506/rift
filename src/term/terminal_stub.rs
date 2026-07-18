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
mod tests {
    use super::*;

    #[test]
    fn new_always_errs() {
        let result = Terminal::new(24, 80, None);
        assert!(result.is_err());
    }

    #[test]
    fn new_errs_regardless_of_shell_cmd() {
        let result = Terminal::new(1, 1, Some("bash".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn resize_updates_size() {
        let mut term = Terminal {
            size: (24, 80),
            name: "stub".to_string(),
        };
        term.resize(30, 100).unwrap();
        assert_eq!(term.size, (30, 100));
    }

    #[test]
    fn write_is_a_silent_no_op() {
        let mut term = Terminal {
            size: (24, 80),
            name: "stub".to_string(),
        };
        assert!(term.write(b"ignored").is_ok());
    }

    #[test]
    fn scroll_methods_are_no_ops() {
        let term = Terminal {
            size: (24, 80),
            name: "stub".to_string(),
        };
        term.scroll_display(5);
        term.scroll_to_bottom();
    }

    #[test]
    fn read_screen_is_empty() {
        let term = Terminal {
            size: (24, 80),
            name: "stub".to_string(),
        };
        let (text, cursor_row, cursor_col, spans) = term.read_screen();
        assert_eq!(text, "");
        assert_eq!(cursor_row, 0);
        assert_eq!(cursor_col, 0);
        assert!(spans.is_empty());
    }
}
