use crate::color::theme::Theme;
use crate::color::Color;
use crate::command::Command;
use crate::command_line::commands::completion::CompletionCandidate;
use crate::document::LineEnding;
use crate::error::manager::ErrorManager;
use crate::error::RiftError;
use crate::floating_window::BorderChars;
use crate::key::Key;
use crate::notification::NotificationType;
use crate::search::SearchMatch;

#[derive(Debug, Clone)]
pub(crate) struct CommandLineWindowSettings {
    pub(crate) width_ratio: f64,
    pub(crate) min_width: usize,
    pub(crate) height: usize,
    pub(crate) border: bool,
    pub(crate) reverse_video: bool,
}

impl Default for CommandLineWindowSettings {
    fn default() -> Self {
        Self {
            width_ratio: 0.6, // 60% of terminal width
            min_width: 40,
            height: 3, // top border (1) + content (1) + bottom border (1)
            border: true,
            reverse_video: false,
        }
    }
}
impl CommandLineWindowSettings {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusLineSegments {
    pub(crate) show_filename: bool,
    pub(crate) show_dirty_indicator: bool,
}

impl Default for StatusLineSegments {
    fn default() -> Self {
        Self {
            show_filename: true,
            show_dirty_indicator: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusLineSettings {
    pub(crate) show_status_line: bool,
    pub(crate) reverse_video: bool,
    pub(crate) segments: StatusLineSegments,
}

impl Default for StatusLineSettings {
    fn default() -> Self {
        Self {
            show_status_line: true,
            reverse_video: false,
            segments: StatusLineSegments::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EditingSettings {
    #[cfg(test)]
    pub(crate) expand_tabs: bool,
    pub(crate) ghost_cut: bool,
    pub(crate) quit_on_q: bool,
}

impl Default for EditingSettings {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            expand_tabs: true,
            ghost_cut: true,
            quit_on_q: true,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LspSettings {
    pub(crate) debug_log: bool,
    pub(crate) virtual_text: bool,
    pub(crate) diagnostic_tooltip: bool,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            debug_log: false,
            virtual_text: true,
            diagnostic_tooltip: true,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UndoSettings {
    pub(crate) persistent: bool,
    pub(crate) dir: Option<std::path::PathBuf>,
}

impl Default for UndoSettings {
    fn default() -> Self {
        Self {
            persistent: true,
            dir: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UserSettings {
    pub(crate) show_line_numbers: bool,
    pub(crate) default_border_chars: Option<BorderChars>,
    pub(crate) command_line_window: CommandLineWindowSettings,
    pub(crate) status_line: StatusLineSettings,
    pub(crate) editor_bg: Option<Color>,
    pub(crate) editor_fg: Option<Color>,
    pub(crate) theme: Option<String>,
    pub(crate) poll_timeout_ms: u64,
    #[cfg(test)]
    pub(crate) tab_width: usize,
    pub(crate) editing: EditingSettings,
    pub(crate) syntax_colors: Option<crate::color::theme::SyntaxColors>,
    pub(crate) soft_wrap: bool,
    pub(crate) wrap_width: Option<usize>,
    pub(crate) clipboard_ring_size: usize,
    pub(crate) equalize_proportional: bool,
    pub(crate) cursor_color: Option<Color>,
    pub(crate) cursor_speed: f64,
    pub(crate) lsp: LspSettings,
    pub(crate) undo: UndoSettings,
}

impl UserSettings {
    #[must_use]
    pub(crate) fn new() -> Self {
        let mut settings = Self {
            show_line_numbers: true,    // Default to showing line numbers
            default_border_chars: None, // None means use FloatingWindow defaults
            command_line_window: CommandLineWindowSettings::default(),
            status_line: StatusLineSettings::default(),
            editor_bg: None,
            editor_fg: None,
            theme: None,
            poll_timeout_ms: 16,
            #[cfg(test)]
            tab_width: 4,
            editing: EditingSettings::default(),
            syntax_colors: None,
            soft_wrap: true,
            wrap_width: None,
            clipboard_ring_size: crate::clipboard::DEFAULT_RING_CAPACITY,
            equalize_proportional: false,
            cursor_color: None,
            cursor_speed: 0.8,
            lsp: LspSettings::default(),
            undo: UndoSettings::default(),
        };

        let default_theme = Theme::gruvbox();
        settings.apply_theme(&default_theme);

        settings
    }

    fn apply_theme(&mut self, theme: &Theme) {
        theme.apply_to_settings(self);
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        Self::new()
    }
}

const COMPLETION_MAX_VISIBLE: usize = 8;

#[derive(Debug, Clone)]
pub(crate) struct CompletionSession {
    pub(crate) candidates: Vec<CompletionCandidate>,
    pub(crate) selected: Option<usize>,
    pub(crate) dropdown_open: bool,
    pub(crate) scroll_offset: usize,
    pub(crate) token_start: usize,
}

impl CompletionSession {
    #[must_use]
    pub(crate) const fn new(candidates: Vec<CompletionCandidate>, token_start: usize) -> Self {
        Self {
            candidates,
            selected: None,
            dropdown_open: false,
            scroll_offset: 0,
            token_start,
        }
    }

    pub(crate) fn select_next(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            None => 0,
            Some(i) => (i + 1) % self.candidates.len(),
        });
        self.ensure_selected_visible();
    }

    pub(crate) fn select_prev(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        let len = self.candidates.len();
        self.selected = Some(match self.selected {
            None => len.saturating_sub(1),
            Some(0) => len - 1,
            Some(i) => i - 1,
        });
        self.ensure_selected_visible();
    }

    #[must_use]
    pub(crate) fn selected_text(&self) -> Option<&str> {
        self.selected
            .and_then(|i| self.candidates.get(i))
            .map(|c| c.text.as_str())
    }

    fn ensure_selected_visible(&mut self) {
        let Some(sel) = self.selected else { return };
        let max_visible = COMPLETION_MAX_VISIBLE.min(self.candidates.len());
        if max_visible == 0 {
            return;
        }
        if sel < self.scroll_offset {
            self.scroll_offset = sel;
        } else if sel >= self.scroll_offset + max_visible {
            self.scroll_offset = sel + 1 - max_visible;
        }
    }
}

pub struct State {
    pub(crate) settings: UserSettings,
    pub(crate) debug_mode: bool,
    pub(crate) file_path: Option<String>,
    pub(crate) file_name: String,
    pub(crate) last_keypress: Option<Key>,
    last_command: Option<Command>,
    pub(crate) cursor_pos: (usize, usize),
    pub(crate) total_lines: usize,
    pub(crate) gutter_width: usize,
    buffer_size: usize,
    pub(crate) command_line: String,
    pub(crate) command_line_cursor: usize,
    pub(crate) is_dirty: bool,
    line_ending: LineEnding,
    pub(crate) error_manager: ErrorManager,
    pub(crate) last_find_char: Option<(char, bool, bool)>,
    pub(crate) last_search_query: Option<String>,
    pub(crate) search_matches: Vec<SearchMatch>,
    pub(crate) command_history: crate::history::command::CommandHistory,
    pub(crate) search_history: crate::history::command::CommandHistory,
    pub(crate) completion_session: Option<CompletionSession>,
    pub(crate) lsp_status: Option<String>,
    pub(crate) is_remote: bool,
}

const GUTTER_PADDING: usize = 2;

/// Columns the line-number gutter needs for `total_lines` lines, padding included.
#[must_use]
fn gutter_width_for(total_lines: usize) -> usize {
    if total_lines == 0 {
        return 0;
    }
    let digits = usize::try_from(total_lines.ilog10()).unwrap_or(0) + 1;
    digits + GUTTER_PADDING
}

impl State {
    #[must_use]
    pub fn new() -> Self {
        Self {
            settings: UserSettings::new(),
            debug_mode: false,
            file_path: None,
            file_name: "[No Name]".to_string(),
            last_keypress: None,
            last_command: None,
            cursor_pos: (0, 0),
            total_lines: 1,
            gutter_width: 3,
            buffer_size: 0,
            command_line: String::new(),
            command_line_cursor: 0,
            is_dirty: false,
            line_ending: LineEnding::LF,
            error_manager: ErrorManager::new(),
            last_find_char: None,
            last_search_query: None,
            search_matches: Vec::new(),
            command_history: crate::history::command::CommandHistory::default(),
            search_history: crate::history::command::CommandHistory::default(),
            completion_session: None,
            lsp_status: None,
            is_remote: false,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_settings(settings: UserSettings) -> Self {
        Self {
            settings,
            debug_mode: false,
            file_path: None,
            file_name: "[No Name]".to_string(),
            last_keypress: None,
            last_command: None,
            cursor_pos: (0, 0),
            total_lines: 1,
            gutter_width: 3,
            buffer_size: 0,
            command_line: String::new(),
            command_line_cursor: 0,
            is_dirty: false,
            line_ending: LineEnding::LF,
            error_manager: ErrorManager::new(),
            last_find_char: None,
            last_search_query: None,
            search_matches: Vec::new(),
            command_history: crate::history::command::CommandHistory::default(),
            search_history: crate::history::command::CommandHistory::default(),
            completion_session: None,
            lsp_status: None,
            is_remote: false,
        }
    }

    #[cfg(test)]
    const fn set_default_border_chars(&mut self, border_chars: Option<BorderChars>) {
        self.settings.default_border_chars = border_chars;
    }

    pub(crate) fn set_file_path(&mut self, path: Option<String>) {
        self.file_path = path;
    }

    pub(crate) const fn toggle_debug(&mut self) {
        self.debug_mode = !self.debug_mode;
    }

    pub(crate) const fn update_keypress(&mut self, key: Key) {
        self.last_keypress = Some(key);
    }

    pub(crate) const fn update_command(&mut self, cmd: Command) {
        self.last_command = Some(cmd);
    }

    pub(crate) const fn update_cursor(&mut self, line: usize, col: usize) {
        self.cursor_pos = (line, col);
    }

    pub(crate) fn update_buffer_stats(
        &mut self,
        total_lines: usize,
        buffer_size: usize,
        line_ending: LineEnding,
    ) {
        self.gutter_width = gutter_width_for(total_lines);

        self.total_lines = total_lines;
        self.buffer_size = buffer_size;
        self.line_ending = line_ending;
    }

    /// Line numbers draw only when both the document and the global setting allow them.
    #[must_use]
    pub(crate) const fn line_numbers_on(&self, doc_enabled: bool) -> bool {
        doc_enabled && self.settings.show_line_numbers
    }

    /// Gutter columns of the active document, 0 when its line numbers are hidden.
    #[must_use]
    pub(crate) const fn gutter_cols(&self, doc_enabled: bool) -> usize {
        if self.line_numbers_on(doc_enabled) {
            self.gutter_width
        } else {
            0
        }
    }

    /// Gutter columns of any document with `total_lines` lines, 0 when hidden.
    #[must_use]
    pub(crate) fn doc_gutter_cols(&self, doc_enabled: bool, total_lines: usize) -> usize {
        if self.line_numbers_on(doc_enabled) {
            gutter_width_for(total_lines)
        } else {
            0
        }
    }

    pub(crate) fn append_to_command_line(&mut self, ch: char) {
        if self.command_line_cursor >= self.command_line.len() {
            self.command_line.push(ch);
        } else {
            self.command_line.insert(self.command_line_cursor, ch);
        }
        self.command_line_cursor += ch.len_utf8();
    }

    pub(crate) fn remove_from_command_line(&mut self) {
        if self.command_line_cursor > 0 {
            let prev_len = self
                .command_line
                .get(..self.command_line_cursor)
                .unwrap_or_default()
                .chars()
                .next_back()
                .map_or(0, char::len_utf8);
            self.command_line
                .remove(self.command_line_cursor - prev_len);
            self.command_line_cursor -= prev_len;
        }
    }

    pub(crate) fn delete_forward_command_line(&mut self) {
        if self.command_line_cursor < self.command_line.len() {
            self.command_line.remove(self.command_line_cursor);
        }
    }

    pub(crate) fn clear_command_line(&mut self) {
        self.command_line.clear();
        self.command_line_cursor = 0;
    }

    pub(crate) fn move_command_line_left(&mut self) {
        let prev_len = self
            .command_line
            .get(..self.command_line_cursor)
            .unwrap_or_default()
            .chars()
            .next_back()
            .map_or(0, char::len_utf8);
        self.command_line_cursor -= prev_len;
    }

    pub(crate) fn move_command_line_right(&mut self) {
        let next_len = self
            .command_line
            .get(self.command_line_cursor..)
            .unwrap_or_default()
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        self.command_line_cursor += next_len;
    }

    pub(crate) const fn move_command_line_home(&mut self) {
        self.command_line_cursor = 0;
    }

    fn command_line_word_target(&self, motion: fn(&str, usize) -> usize) -> usize {
        let line = &self.command_line;
        let cursor = self.command_line_cursor.min(line.len());
        let char_cursor = line.get(..cursor).map_or(0, |s| s.chars().count());
        let target = motion(line, char_cursor);

        line.char_indices()
            .nth(target)
            .map_or(line.len(), |(byte, _)| byte)
    }

    pub(crate) fn move_command_line_word_left(&mut self) {
        self.command_line_cursor =
            self.command_line_word_target(crate::movement::boundaries::prev_word);
    }

    pub(crate) fn move_command_line_word_right(&mut self) {
        self.command_line_cursor =
            self.command_line_word_target(crate::movement::boundaries::next_word);
    }

    pub(crate) fn delete_word_back_command_line(&mut self) {
        let start = self.command_line_word_target(crate::movement::boundaries::prev_word);
        if start < self.command_line_cursor {
            self.command_line
                .replace_range(start..self.command_line_cursor, "");
            self.command_line_cursor = start;
        }
    }

    pub(crate) const fn move_command_line_end(&mut self) {
        self.command_line_cursor = self.command_line.len();
    }

    pub(crate) fn handle_error(&mut self, err: RiftError) {
        self.error_manager.handle(err);
    }

    pub(crate) fn update_filename(&mut self, filename: String) {
        self.file_name = filename;
    }

    pub(crate) fn notify(&mut self, kind: NotificationType, message: impl Into<String>) {
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

    pub(crate) const fn update_dirty(&mut self, is_dirty: bool) {
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
