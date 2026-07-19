//! Shared testing helpers and mocks (e.g. `MockTerminal`), available to any
//! test module in the crate via `use crate::test_utils::MockTerminal;`.

/// Test utilities introduce no production-only behavior; tests assert
/// invariants, not implementation details.
use crate::key::Key;
use crate::term::{CursorShape, Size, TerminalBackend};

/// Mock `TerminalBackend` for testing; records writes, cursor moves, and
/// clear-screen calls for later assertion.
pub struct MockTerminal {
    pub writes: Vec<Vec<u8>>,
    pub cursor_moves: Vec<(u16, u16)>,
    pub clear_screen_calls: usize,
    pub size: (u16, u16),
}

impl MockTerminal {
    /// Create a new mock terminal with specified dimensions
    pub fn new(rows: u16, cols: u16) -> Self {
        MockTerminal {
            writes: Vec::new(),
            cursor_moves: Vec::new(),
            clear_screen_calls: 0,
            size: (rows, cols),
        }
    }

    /// Get all written bytes as a single vector
    pub fn get_written_bytes(&self) -> Vec<u8> {
        self.writes.iter().flatten().cloned().collect()
    }

    /// Get all written bytes as a string (lossy UTF-8 conversion)
    pub fn get_written_string(&self) -> String {
        String::from_utf8_lossy(&self.get_written_bytes()).to_string()
    }

    /// Clear all recorded operations (useful for testing multiple renders)
    pub fn clear(&mut self) {
        self.writes.clear();
        self.cursor_moves.clear();
        self.clear_screen_calls = 0;
    }
}

impl TerminalBackend for MockTerminal {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn deinit(&mut self) {}

    fn poll(&mut self, _duration: std::time::Duration) -> Result<bool, String> {
        Ok(false)
    }

    fn read_key(&mut self) -> Result<Option<Key>, String> {
        Err("Not implemented in mock".to_string())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writes.push(bytes.to_vec());
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
        self.clear_screen_calls += 1;
        Ok(())
    }

    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String> {
        self.cursor_moves.push((row, col));
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

    fn set_cursor_shape(&mut self, _shape: CursorShape) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
