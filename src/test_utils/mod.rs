//! Test utilities
//! Shared testing helpers and mocks
//!
//! # Usage
//!
//! Import `MockTerminal` in any test file (including nested test modules):
//!
//! ```rust,no_run
//! use crate::test_utils::MockTerminal;
//! use crate::term::TerminalBackend;
//!
//! #[test]
//! fn my_test() {
//!     let mut term = MockTerminal::new(10, 80);
//!     // Use term as a TerminalBackend
//!     term.write(b"hello").unwrap();
//!
//!     // Check what was written
//!     let written = term.get_written_string();
//!     assert!(written.contains("hello"));
//!
//!     // Check cursor moves
//!     term.move_cursor(5, 10).unwrap();
//!     assert_eq!(term.cursor_moves.len(), 1);
//!     assert_eq!(term.cursor_moves[0], (5, 10));
//!
//!     // Check clear screen calls
//!     term.clear_screen().unwrap();
//!     assert_eq!(term.clear_screen_calls, 1);
//!
//!     // Reset for multiple test operations
//!     term.clear();
//! }
//! ```
//!
//! # Available from any test module
//!
//! The `test_utils` module is available to all test modules in the crate.
//! You can import it from:
//! - Top-level test modules: `use crate::test_utils::MockTerminal;`
//! - Nested test modules: `use super::super::test_utils::MockTerminal;` or `use crate::test_utils::MockTerminal;`
//! - Integration tests: `use rift::test_utils::MockTerminal;` (if exposed)

/// ## test_utils/ Invariants
///
/// - Test utilities introduce no production-only behavior.
/// - Tests assert invariants, not implementation details.
/// - Buffer and executor logic are testable without a terminal.
/// - Boundary and edge cases are explicitly tested.
use crate::key::Key;
use crate::term::{Size, TerminalBackend};

/// Mock terminal backend for testing
///
/// Records all terminal operations for verification in tests.
/// Implements `TerminalBackend` trait and tracks:
/// - All write operations
/// - Cursor movements
/// - Clear screen calls
///
/// # Example
///
/// ```rust,no_run
/// use crate::test_utils::MockTerminal;
/// use crate::term::TerminalBackend;
///
/// let mut term = MockTerminal::new(24, 80);
/// term.write(b"test").unwrap();
/// assert_eq!(term.get_written_string(), "test");
/// ```
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

    fn read_key(&mut self) -> Result<Key, String> {
        Err("Not implemented in mock".to_string())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writes.push(bytes.to_vec());
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
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
