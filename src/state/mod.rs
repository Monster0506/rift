//! Editor state management
//! Centralized state for editor settings, debug mode, and runtime information

use crate::color::{Color, Theme};
use crate::command::Command;
/// ## state/ Invariants
///
/// - Editor mode is explicit and globally consistent.
/// - State transitions occur only through well-defined control flow.
/// - There is exactly one active buffer at a time in v0.
/// - Editor state is never partially updated.
/// - State changes are observable by the renderer but never influenced by it.
use crate::document::LineEnding;
use crate::error::manager::ErrorManager;
use crate::error::RiftError;
use crate::floating_window::BorderChars;
use crate::key::Key;
use crate::notification::NotificationType;
use crate::search::{SearchDirection, SearchMatch};

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

/// Status line settings
#[derive(Debug, Clone)]
pub struct StatusLineSettings {
    /// Whether to show the status line at all
    pub show_status_line: bool,
    /// Whether to show filename in status bar (normal mode)
    pub show_filename: bool,
    /// Whether to show dirty state indicator (*) in status bar
    pub show_dirty_indicator: bool,
    /// Whether to use reverse video for status bar
    pub reverse_video: bool,
}

impl Default for StatusLineSettings {
    /// Create default status line settings
    fn default() -> Self {
        StatusLineSettings {
            show_status_line: true,
            show_filename: true,
            show_dirty_indicator: true,
            reverse_video: false,
        }
    }
}

/// User settings that persist across sessions
/// These are preferences that should be saved and loaded from a config file
#[derive(Debug, Clone)]
pub struct UserSettings {
    /// Whether to show line numbers
    pub show_line_numbers: bool,
    /// Default border characters for floating windows
    pub default_border_chars: Option<BorderChars>,
    /// Command line window settings
    pub command_line_window: CommandLineWindowSettings,
    /// Status line settings
    pub status_line: StatusLineSettings,
    /// Editor background color (None means use terminal default)
    pub editor_bg: Option<Color>,
    /// Editor foreground color (None means use terminal default)
    pub editor_fg: Option<Color>,
    /// Current theme name (None means no theme applied)
    pub theme: Option<String>,
    /// Main loop poll timeout in milliseconds
    pub poll_timeout_ms: u64,
    /// Tab width in spaces
    pub tab_width: usize,
    /// Whether to expand tabs to spaces
    pub expand_tabs: bool,
    /// Optional syntax highlighting colors from current theme
    pub syntax_colors: Option<crate::color::theme::SyntaxColors>,
}

impl UserSettings {
    /// Create default user settings
    #[must_use]
    pub fn new() -> Self {
        UserSettings {
            show_line_numbers: true,    // Default to showing line numbers
            default_border_chars: None, // None means use FloatingWindow defaults
            command_line_window: CommandLineWindowSettings::default(),
            status_line: StatusLineSettings::default(),
            editor_bg: None,
            editor_fg: None,
            theme: None,
            poll_timeout_ms: 16,
            tab_width: 4,
            expand_tabs: true,
            syntax_colors: None,
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
    /// Current gutter width (cached for optimization)
    pub gutter_width: usize,
    /// Threshold at which gutter width must increase
    pub next_gutter_threshold: usize,
    /// Buffer size
    pub buffer_size: usize,
    /// Command line input (for command mode)
    pub command_line: String,
    /// Command line cursor position (index in bytes)
    pub command_line_cursor: usize,
    /// Whether the current document has unsaved changes
    pub is_dirty: bool,
    /// Line ending type of the current document
    pub line_ending: LineEnding,
    /// Error and notification manager
    pub error_manager: ErrorManager,
    /// Last search query
    pub last_search_query: Option<String>,
    /// Search direction
    pub search_direction: SearchDirection,
    /// Search matches
    pub search_matches: Vec<SearchMatch>,
    /// Overlay content for Mode::Overlay (left/right panes)
    pub overlay_content: Option<OverlayContent>,
}

/// Content for split-view overlay (used in Mode::Overlay)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayContent {
    /// Left pane content (lines)
    pub left: Vec<Vec<char>>,
    /// Right pane content (lines)
    pub right: Vec<Vec<char>>,
    /// Left pane width percentage (0-100)
    pub left_width_percent: u8,
    /// Cursor position (line index in left pane)
    pub cursor: usize,
    /// Which lines are selectable (for skipping connector lines)
    pub selectable: Vec<bool>,
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
            gutter_width: 2,
            next_gutter_threshold: 10,
            buffer_size: 0,
            command_line: String::new(),
            command_line_cursor: 0,
            is_dirty: false,
            line_ending: LineEnding::LF,
            error_manager: ErrorManager::new(),
            last_search_query: None,
            search_direction: SearchDirection::Forward,
            search_matches: Vec::new(),
            overlay_content: None,
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
            gutter_width: 2,
            next_gutter_threshold: 10,
            buffer_size: 0,
            command_line: String::new(),
            command_line_cursor: 0,
            is_dirty: false,
            line_ending: LineEnding::LF,
            error_manager: ErrorManager::new(),
            last_search_query: None,
            search_direction: SearchDirection::Forward,
            search_matches: Vec::new(),
            overlay_content: None,
        }
    }

    /// Set default border characters for floating windows
    pub fn set_default_border_chars(&mut self, border_chars: Option<BorderChars>) {
        self.settings.default_border_chars = border_chars;
    }

    /// Set whether to expand tabs to spaces
    pub fn set_expand_tabs(&mut self, expand: bool) {
        self.settings.expand_tabs = expand;
    }

    /// Set tab width
    pub fn set_tab_width(&mut self, width: usize) {
        self.settings.tab_width = width;
    }

    /// Set the current file path
    pub fn set_file_path(&mut self, path: Option<String>) {
        self.file_path = path;
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
    pub fn update_buffer_stats(
        &mut self,
        total_lines: usize,
        buffer_size: usize,
        line_ending: LineEnding,
    ) {
        // If total lines crossed a threshold, update gutter width
        if total_lines >= self.next_gutter_threshold
            || (total_lines < self.next_gutter_threshold / 10 && self.gutter_width > 2)
        {
            // Recalculate gutter width: number of digits + 1
            self.gutter_width = if total_lines == 0 {
                0
            } else {
                total_lines.to_string().len() + 1
            };
            // Set next threshold to next power of 10
            let mut threshold = 10;
            while threshold <= total_lines {
                threshold *= 10;
            }
            self.next_gutter_threshold = threshold;
        }

        self.total_lines = total_lines;
        self.buffer_size = buffer_size;
        self.line_ending = line_ending;
    }

    /// Append a character to the command line at cursor position
    pub fn append_to_command_line(&mut self, ch: char) {
        if self.command_line_cursor >= self.command_line.len() {
            self.command_line.push(ch);
        } else {
            self.command_line.insert(self.command_line_cursor, ch);
        }
        self.command_line_cursor += 1;
    }

    /// Remove character before cursor (Backspace)
    pub fn remove_from_command_line(&mut self) {
        if self.command_line_cursor > 0 {
            // Check if we are at end or middle
            if self.command_line_cursor >= self.command_line.len() {
                self.command_line.pop();
            } else {
                self.command_line.remove(self.command_line_cursor - 1);
            }
            self.command_line_cursor -= 1;
        }
    }

    /// Delete character at cursor (Delete)
    pub fn delete_forward_command_line(&mut self) {
        if self.command_line_cursor < self.command_line.len() {
            self.command_line.remove(self.command_line_cursor);
        }
    }

    /// Clear the command line
    pub fn clear_command_line(&mut self) {
        self.command_line.clear();
        self.command_line_cursor = 0;
    }

    /// Move command line cursor left
    pub fn move_command_line_left(&mut self) {
        self.command_line_cursor = self.command_line_cursor.saturating_sub(1);
    }

    /// Move command line cursor right
    pub fn move_command_line_right(&mut self) {
        if self.command_line_cursor < self.command_line.len() {
            self.command_line_cursor += 1;
        }
    }

    /// Move command line cursor to start
    pub fn move_command_line_home(&mut self) {
        self.command_line_cursor = 0;
    }

    /// Move command line cursor to end
    pub fn move_command_line_end(&mut self) {
        self.command_line_cursor = self.command_line.len();
    }

    /// Handle a RiftError by delegating to the ErrorManager
    pub fn handle_error(&mut self, err: RiftError) {
        self.error_manager.handle(err);
    }

    /// Update filename for display (should match Document's display_name)
    pub fn update_filename(&mut self, filename: String) {
        self.file_name = filename;
    }

    /// Add a notification
    pub fn notify(&mut self, kind: NotificationType, message: impl Into<String>) {
        // Notifications are ephemeral by default unless error
        let ttl = match kind {
            NotificationType::Error => Some(std::time::Duration::from_secs(10)),
            NotificationType::Warning => Some(std::time::Duration::from_secs(8)),
            NotificationType::Info => Some(std::time::Duration::from_secs(5)),
            NotificationType::Success => Some(std::time::Duration::from_secs(3)),
        };
        self.error_manager
            .notifications_mut()
            .add(kind, message, ttl);
    }

    /// Update dirty state
    pub fn update_dirty(&mut self, is_dirty: bool) {
        self.is_dirty = is_dirty;
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
