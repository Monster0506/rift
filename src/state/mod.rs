//! Editor state management
//! Centralized state for editor settings, debug mode, and runtime information

use crate::color::{Color, Theme};
use crate::command::Command;
use crate::floating_window::BorderChars;
/// ## state/ Invariants
///
/// - Editor mode is explicit and globally consistent.
/// - State transitions occur only through well-defined control flow.
/// - There is exactly one active buffer at a time in v0.
/// - Editor state is never partially updated.
/// - State changes are observable by the renderer but never influenced by it.
use crate::key::Key;

/// Command line window settings
#[derive(Debug, Clone)]
pub struct CommandLineWindowSettings {
    /// Width as a ratio of terminal width (0.0 to 1.0)
    pub width_ratio: f64,
    /// Minimum width in columns
    pub min_width: usize,
    /// Height in rows (including borders)
    pub height: usize,
    /// Whether to draw a border around the window
    pub border: bool,
    /// Whether to use reverse video (inverted colors)
    pub reverse_video: bool,
}

impl Default for CommandLineWindowSettings {
    /// Create default command line window settings
    fn default() -> Self {
        CommandLineWindowSettings {
            width_ratio: 0.6, // 60% of terminal width
            min_width: 40,
            height: 3, // top border (1) + content (1) + bottom border (1)
            border: true,
            reverse_video: false,
        }
    }
}
impl CommandLineWindowSettings {}

/// User settings that persist across sessions
/// These are preferences that should be saved and loaded from a config file
#[derive(Debug, Clone)]
pub struct UserSettings {
    /// Whether to expand tabs to spaces when inserting
    pub expand_tabs: bool,
    /// Tab width in spaces (for display and expansion)
    pub tab_width: usize,
    /// Default border characters for floating windows
    pub default_border_chars: Option<BorderChars>,
    /// Command line window settings
    pub command_line_window: CommandLineWindowSettings,
    /// Editor background color (None means use terminal default)
    pub editor_bg: Option<Color>,
    /// Editor foreground color (None means use terminal default)
    pub editor_fg: Option<Color>,
    /// Current theme name (None means no theme applied)
    pub theme: Option<String>,
}

impl UserSettings {
    /// Create default user settings
    #[must_use]
    pub fn new() -> Self {
        UserSettings {
            expand_tabs: true,          // Default to expanding tabs to spaces
            tab_width: 4,               // Default tab width
            default_border_chars: None, // None means use FloatingWindow defaults
            command_line_window: CommandLineWindowSettings::default(),
            editor_bg: None,
            editor_fg: None,
            theme: None,
        }
    }

    /// Apply a theme to the settings using the theme handler
    /// This delegates to the theme handler which can apply all theme properties
    pub fn apply_theme(&mut self, theme: &Theme) {
        theme.apply_to_settings(self);
    }

    /// Get the current theme name
    #[must_use]
    pub fn get_theme_name(&self) -> Option<&str> {
        self.theme.as_deref()
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        Self::new()
    }
}

/// Editor runtime state (session-specific, not persisted)
pub struct State {
    /// User settings (persistent preferences)
    pub settings: UserSettings,
    /// Whether debug mode is enabled (session-only, does not persist)
    pub debug_mode: bool,
    /// Current file path (None if no file loaded)
    pub file_path: Option<String>,
    /// Cached filename for display
    pub file_name: String,
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
    /// Command execution error message (if any)
    pub command_error: Option<String>,
}

impl State {
    /// Create a new state instance with default values
    #[must_use]
    pub fn new() -> Self {
        State {
            settings: UserSettings::new(),
            debug_mode: false,
            file_path: None,
            file_name: "[No Name]".to_string(),
            last_keypress: None,
            last_command: None,
            cursor_pos: (0, 0),
            total_lines: 1,
            buffer_size: 0,
            command_line: String::new(),
            command_error: None,
        }
    }

    /// Create a new state instance with custom user settings
    #[must_use]
    pub fn with_settings(settings: UserSettings) -> Self {
        State {
            settings,
            debug_mode: false,
            file_path: None,
            file_name: "[No Name]".to_string(),
            last_keypress: None,
            last_command: None,
            cursor_pos: (0, 0),
            total_lines: 1,
            buffer_size: 0,
            command_line: String::new(),
            command_error: None,
        }
    }

    /// Set default border characters for floating windows
    pub fn set_default_border_chars(&mut self, border_chars: Option<BorderChars>) {
        self.settings.default_border_chars = border_chars;
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
        self.settings.expand_tabs = expand;
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

    /// Set command error message
    pub fn set_command_error(&mut self, error: Option<String>) {
        self.command_error = error;
    }

    /// Update filename for display (should match Document's display_name)
    pub fn update_filename(&mut self, filename: String) {
        self.file_name = filename;
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
