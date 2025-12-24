//! Editor state management
//! Centralized state for editor settings, debug mode, and runtime information


/// ## state/ Invariants
///
/// - Editor mode is explicit and globally consistent.
/// - State transitions occur only through well-defined control flow.
/// - There is exactly one active buffer at a time in v0.
/// - Editor state is never partially updated.
/// - State changes are observable by the renderer but never influenced by it.
use crate::key::Key;
use crate::command::Command;
use crate::floating_window::BorderChars;

/// Editor state containing settings and runtime information
pub struct State {
    /// Whether debug mode is enabled
    pub debug_mode: bool,
    /// Whether to expand tabs to spaces when inserting
    pub expand_tabs: bool,
    /// Current file path (None if no file loaded)
    pub file_path: Option<String>,
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
    /// Command line input (for command mode)
    pub command_line: String,
    /// Default border characters for floating windows
    pub default_border_chars: Option<BorderChars>,
}

impl State {
    /// Create a new state instance with default values
    pub fn new() -> Self {
        State {
            debug_mode: false,
            expand_tabs: true, // Default to expanding tabs to spaces
            file_path: None,
            last_keypress: None,
            last_command: None,
            cursor_pos: (0, 0),
            total_lines: 1,
            buffer_size: 0,
            command_line: String::new(),
            default_border_chars: None, // None means use FloatingWindow defaults
        }
    }

    /// Set default border characters for floating windows
    pub fn set_default_border_chars(&mut self, border_chars: Option<BorderChars>) {
        self.default_border_chars = border_chars;
    }

    /// Set the current file path
    pub fn set_file_path(&mut self, path: Option<String>) {
        self.file_path = path;
    }

    /// Toggle debug mode
    pub fn toggle_debug(&mut self) {
        self.debug_mode = !self.debug_mode;
    }

    /// Set whether to expand tabs to spaces
    pub fn set_expand_tabs(&mut self, expand: bool) {
        self.expand_tabs = expand;
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

    /// Append a character to the command line
    pub fn append_to_command_line(&mut self, ch: char) {
        self.command_line.push(ch);
    }

    /// Remove the last character from the command line (backspace)
    pub fn remove_from_command_line(&mut self) {
        self.command_line.pop();
    }

    /// Clear the command line
    pub fn clear_command_line(&mut self) {
        self.command_line.clear();
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

