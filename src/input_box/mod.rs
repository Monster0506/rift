//! Input Box Component
//!
//! A text input component that supports:
//! - Typing and editing
//! - Cursor navigation (char, word, line)
//! - Horizontal scrolling
//! - Input validation
//! - Masking (password mode)
//! - Placeholders
//! - Max length constraints

use crate::character::Character;
use crate::color::Color;
use crate::command::input::{self, Direction, Granularity, InputIntent};
use crate::component::{Component, EventResult};
use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
use crate::key::Key;
use crate::layer::{Cell, Layer};

/// Configuration for InputBox
#[derive(Clone)]
pub struct InputBoxConfig {
    /// Placeholder text when empty
    pub placeholder: Option<String>,
    /// Mask character for secure input (e.g. '*')
    pub mask_char: Option<char>,
    /// Maximum character length
    pub max_len: Option<usize>,
    /// Window title
    pub title: Option<String>,
    /// Width of the input box (content width)
    pub width: usize,
    /// Custom validation function
    pub validator: Option<std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>>,
}

impl Default for InputBoxConfig {
    fn default() -> Self {
        Self {
            placeholder: None,
            mask_char: None,
            max_len: None,
            title: None,
            width: 40,
            validator: None,
        }
    }
}

/// A generic input box component
pub struct InputBox {
    /// Current text content
    content: String,
    /// Cursor position (character index)
    cursor_idx: usize,
    /// Horizontal scroll offset (character index)
    scroll_offset: usize,
    /// Configuration
    config: InputBoxConfig,
    /// Current validation state
    is_valid: bool,

    // Callbacks
    on_submit: Option<Box<dyn FnMut(String) -> EventResult>>,
    on_cancel: Option<Box<dyn FnMut() -> EventResult>>,
    on_change: Option<Box<dyn FnMut(String) -> EventResult>>,

    // Last calculated cursor pos for rendering
    last_cursor_pos: Option<(u16, u16)>,
}

impl InputBox {
    /// Create a new InputBox with default config
    pub fn new() -> Self {
        Self::with_config(InputBoxConfig::default())
    }

    /// Create a new InputBox with specific config
    pub fn with_config(config: InputBoxConfig) -> Self {
        Self {
            content: String::new(),
            cursor_idx: 0,
            scroll_offset: 0,
            config,
            is_valid: true,
            on_submit: None,
            on_cancel: None,
            on_change: None,
            last_cursor_pos: None,
        }
    }

    /// Set initial content
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self.cursor_idx = self.content.chars().count();
        self.validate();
        self
    }

    /// Set submit callback
    pub fn on_submit<F>(mut self, callback: F) -> Self
    where
        F: FnMut(String) -> EventResult + 'static,
    {
        self.on_submit = Some(Box::new(callback));
        self
    }

    /// Set cancel callback
    pub fn on_cancel<F>(mut self, callback: F) -> Self
    where
        F: FnMut() -> EventResult + 'static,
    {
        self.on_cancel = Some(Box::new(callback));
        self
    }

    /// Set change callback
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(String) -> EventResult + 'static,
    {
        self.on_change = Some(Box::new(callback));
        self
    }

    /// Update internal validation state
    fn validate(&mut self) {
        if let Some(validator) = &self.config.validator {
            self.is_valid = validator(&self.content);
        } else {
            self.is_valid = true;
        }
    }

    /// Handle text insertion
    fn insert_char(&mut self, c: char) {
        if let Some(max) = self.config.max_len {
            if self.content.chars().count() >= max {
                return;
            }
        }

        if self.cursor_idx >= self.content.chars().count() {
            self.content.push(c);
        } else {
            // Need to handle char indices vs byte indices implies we might want to work with Vec<char> strictly or be careful
            // For now, let's convert to chars, insert, convert back. Not efficient but safe for MVP.
            let mut chars: Vec<char> = self.content.chars().collect();
            chars.insert(self.cursor_idx, c);
            self.content = chars.into_iter().collect();
        }
        self.cursor_idx += 1;
        self.validate();
        self.trigger_change();
    }

    /// Handle text deletion
    fn delete_char(&mut self, dir: Direction) {
        let len = self.content.chars().count();
        if len == 0 {
            return;
        }

        let mut chars: Vec<char> = self.content.chars().collect();

        match dir {
            Direction::Left => {
                if self.cursor_idx > 0 {
                    chars.remove(self.cursor_idx - 1);
                    self.cursor_idx -= 1;
                } else {
                    return;
                }
            }
            Direction::Right => {
                if self.cursor_idx < len {
                    chars.remove(self.cursor_idx);
                } else {
                    return;
                }
            }
            _ => return,
        }

        self.content = chars.into_iter().collect();
        self.validate();
        self.trigger_change();
    }

    fn trigger_change(&mut self) {
        if let Some(cb) = self.on_change.as_mut() {
            cb(self.content.clone());
        }
    }

    fn move_cursor(&mut self, dir: Direction, granularity: Granularity) {
        let len = self.content.chars().count();
        match (dir, granularity) {
            (Direction::Left, Granularity::Character) => {
                self.cursor_idx = self.cursor_idx.saturating_sub(1);
            }
            (Direction::Right, Granularity::Character) => {
                if self.cursor_idx < len {
                    self.cursor_idx += 1;
                }
            }
            (Direction::Left, Granularity::Word) => {
                // Simple word jump: skip until space, then skip spaces
                // Or reuse existing movement logic if available.
                // For MVP self-contained:
                self.cursor_idx =
                    crate::movement::boundaries::prev_word(&self.content, self.cursor_idx);
            }
            (Direction::Right, Granularity::Word) => {
                self.cursor_idx =
                    crate::movement::boundaries::next_word(&self.content, self.cursor_idx);
            }
            (Direction::Left, Granularity::Line) | (Direction::Left, Granularity::Document) => {
                self.cursor_idx = 0;
            }
            (Direction::Right, Granularity::Line) | (Direction::Right, Granularity::Document) => {
                self.cursor_idx = len;
            }
            _ => {}
        }
    }
}

impl Component for InputBox {
    fn handle_input(&mut self, key: Key) -> EventResult {
        // Resolve input using shared logic
        if let Some(intent) = input::resolve_input(key) {
            match intent {
                InputIntent::Type(c) => {
                    self.insert_char(c);
                    EventResult::Consumed
                }
                InputIntent::Delete(dir, _) => {
                    self.delete_char(dir);
                    EventResult::Consumed
                }
                InputIntent::Move(dir, granularity) => {
                    self.move_cursor(dir, granularity);
                    EventResult::Consumed
                }
                InputIntent::Accept => {
                    if let Some(cb) = self.on_submit.as_mut() {
                        cb(self.content.clone())
                    } else {
                        EventResult::Consumed
                    }
                }
                InputIntent::Cancel => {
                    if let Some(cb) = self.on_cancel.as_mut() {
                        cb()
                    } else {
                        EventResult::Consumed
                    }
                }
            }
        } else {
            EventResult::Ignored
        }
    }

    fn render(&mut self, layer: &mut Layer) {
        // Auto-scroll
        let content_width = self.config.width.saturating_sub(2); // Assume border
        if self.cursor_idx < self.scroll_offset {
            self.scroll_offset = self.cursor_idx;
        } else if self.cursor_idx >= self.scroll_offset + content_width {
            self.scroll_offset = self.cursor_idx - content_width + 1;
        }

        // Color validity
        let border_color = if self.is_valid {
            Color::Blue // Default active color
        } else {
            Color::Red
        };

        let style = WindowStyle::new().with_border(true).with_fg(border_color);
        // .with_title(self.config.title.clone()) // FloatingWindow handles title? No, need manual title render or update FW.
        // FloatingWindow currently doesn't support title directly in style, so we render border.

        let window = FloatingWindow::with_style(
            WindowPosition::Center,
            self.config.width,
            3, // Height 3: Top border, Content, Bottom border
            style,
        );

        // Prepare displayed content
        let display_text = if self.content.is_empty() {
            if let Some(ph) = &self.config.placeholder {
                ph.clone()
            } else {
                String::new()
            }
        } else {
            if let Some(mask) = self.config.mask_char {
                std::iter::repeat(mask)
                    .take(self.content.chars().count())
                    .collect()
            } else {
                self.content.clone()
            }
        };

        let visible_slice: String = display_text
            .chars()
            .skip(self.scroll_offset)
            .take(content_width)
            .collect();

        let chars: Vec<char> = visible_slice.chars().collect();
        // Convert to cells
        // If placeholder and empty, use grey?
        let mut cells = Vec::new();
        for c in chars {
            let mut cell = Cell::new(Character::from(c));
            if self.content.is_empty() && self.config.placeholder.is_some() {
                cell.fg = Some(Color::DarkGrey);
            }
            cells.push(cell);
        }

        window.render_cells(layer, &[cells]);

        // Calculate absolute cursor position
        let (win_row, win_col) =
            window.calculate_position(layer.rows() as u16, layer.cols() as u16);

        // Relative cursor visual position
        let visual_cursor_x = self.cursor_idx.saturating_sub(self.scroll_offset);
        if visual_cursor_x < content_width {
            self.last_cursor_pos = Some((
                win_row + 1, // +1 for top border
                win_col + 1 + visual_cursor_x as u16,
            ));
        } else {
            self.last_cursor_pos = None;
        }
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        self.last_cursor_pos
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests;
