//! Editor state management
//! Centralized state for editor settings, debug mode, and runtime information

use crate::key::Key;
use crate::command::Command;

/// Editor state containing settings and runtime information
pub struct State {
    /// Whether debug mode is enabled
    pub debug_mode: bool,
    /// Last keypress received
    pub last_keypress: Option<Key>,
    /// Last command that will be executed
    pub last_command: Option<Command>,
    /// Current cursor position (line, column)
    pub cursor_pos: (usize, usize),
    /// Total number of lines in buffer
    pub total_lines: usize,
    /// Buffer size
    pub buffer_size: usize,
}

impl State {
    /// Create a new state instance with default values
    pub fn new() -> Self {
        State {
            debug_mode: false,
            last_keypress: None,
            last_command: None,
            cursor_pos: (0, 0),
            total_lines: 1,
            buffer_size: 0,
        }
    }

    /// Toggle debug mode
    pub fn toggle_debug(&mut self) {
        self.debug_mode = !self.debug_mode;
    }

    /// Update last keypress
    pub fn update_keypress(&mut self, key: Key) {
        self.last_keypress = Some(key);
    }

    /// Update last command
    pub fn update_command(&mut self, cmd: Command) {
        self.last_command = Some(cmd);
    }

    /// Update cursor position
    pub fn update_cursor(&mut self, line: usize, col: usize) {
        self.cursor_pos = (line, col);
    }

    /// Update buffer statistics
    pub fn update_buffer_stats(&mut self, total_lines: usize, buffer_size: usize) {
        self.total_lines = total_lines;
        self.buffer_size = buffer_size;
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

