//! Reusable split-view overlay component
//!
//! A 90% width/height floating window with left and right panes
//! separated by a vertical divider.

use std::iter::repeat_n;

use crate::character::Character;
use crate::color::Color;
use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
use crate::layer::Layer;

use crate::component::{Component, EventResult};
use crate::key::Key;

/// Vertical divider character
const DIVIDER_CHAR: char = '│';

/// A split-view overlay that renders left and right panes
pub struct SelectView {
    /// Percentage of width allocated to left pane (0-100)
    left_width_percent: u8,
    /// Content for the left pane
    left_content: Vec<Vec<crate::layer::Cell>>,
    /// Content for the right pane
    right_content: Vec<Vec<crate::layer::Cell>>,
    /// Scroll offset for left pane
    left_scroll: usize,
    /// Scroll offset for right pane
    right_scroll: usize,
    /// Selected line index (in left pane)
    selected_line: Option<usize>,
    /// Mask of selectable lines (true = selectable)
    selectable_lines: Vec<bool>,
    /// Foreground color
    fg: Option<Color>,
    /// Background color
    bg: Option<Color>,

    // Callbacks
    on_select: Option<Box<dyn FnMut(usize) -> EventResult>>,
    on_cancel: Option<Box<dyn FnMut() -> EventResult>>,
    on_change: Option<Box<dyn FnMut(usize) -> EventResult>>,
    on_down: Option<Box<dyn FnMut(usize) -> EventResult>>,
    on_up: Option<Box<dyn FnMut(usize) -> EventResult>>,
}

impl SelectView {
    /// Create a new split view with 40% left pane
    pub fn new() -> Self {
        Self {
            left_width_percent: 40,
            left_content: Vec::new(),
            right_content: Vec::new(),
            left_scroll: 0,
            right_scroll: 0,
            selected_line: None,
            selectable_lines: Vec::new(),
            fg: None,
            bg: None,
            on_select: None,
            on_cancel: None,
            on_change: None,
            on_down: None,
            on_up: None,
        }
    }

    /// Set the left pane width percentage
    #[must_use]
    pub fn with_left_width(mut self, percent: u8) -> Self {
        self.left_width_percent = percent.clamp(10, 90);
        self
    }

    /// Set the left pane content
    pub fn set_left_content(&mut self, content: Vec<Vec<crate::layer::Cell>>) {
        self.left_content = content;
    }

    /// Set the right pane content
    pub fn set_right_content(&mut self, content: Vec<Vec<crate::layer::Cell>>) {
        self.right_content = content;
    }

    /// Set left pane scroll offset
    pub fn set_left_scroll(&mut self, scroll: usize) {
        self.left_scroll = scroll;
    }

    /// Set right pane scroll offset
    pub fn set_right_scroll(&mut self, scroll: usize) {
        self.right_scroll = scroll;
    }

    /// Set the selectable lines mask
    #[must_use]
    pub fn with_selectable(mut self, selectable: Vec<bool>) -> Self {
        self.selectable_lines = selectable;
        self
    }

    /// Set foreground and background colors
    #[must_use]
    pub fn with_colors(mut self, fg: Option<Color>, bg: Option<Color>) -> Self {
        self.fg = fg;
        self.bg = bg;
        self
    }

    /// Set callback for selection (Enter)
    #[must_use]
    pub fn on_select<F>(mut self, callback: F) -> Self
    where
        F: FnMut(usize) -> EventResult + 'static,
    {
        self.on_select = Some(Box::new(callback));
        self
    }

    /// Set callback for cancellation (Esc/q)
    #[must_use]
    pub fn on_cancel<F>(mut self, callback: F) -> Self
    where
        F: FnMut() -> EventResult + 'static,
    {
        self.on_cancel = Some(Box::new(callback));
        self
    }

    /// Set callback for selection change
    #[must_use]
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(usize) -> EventResult + 'static,
    {
        self.on_change = Some(Box::new(callback));
        self
    }

    /// Set callback for moving down
    #[must_use]
    pub fn on_down<F>(mut self, callback: F) -> Self
    where
        F: FnMut(usize) -> EventResult + 'static,
    {
        self.on_down = Some(Box::new(callback));
        self
    }

    /// Set callback for moving up
    #[must_use]
    pub fn on_up<F>(mut self, callback: F) -> Self
    where
        F: FnMut(usize) -> EventResult + 'static,
    {
        self.on_up = Some(Box::new(callback));
        self
    }

    /// Handle keyboard input
    pub fn handle_input(&mut self, key: Key) -> EventResult {
        match key {
            Key::Char('q') | Key::Escape => {
                if let Some(cb) = self.on_cancel.as_mut() {
                    cb()
                } else {
                    EventResult::Consumed
                }
            }
            Key::Enter => {
                if let Some(idx) = self.selected_line {
                    if let Some(cb) = self.on_select.as_mut() {
                        cb(idx)
                    } else {
                        EventResult::Consumed
                    }
                } else {
                    EventResult::Ignored
                }
            }
            Key::Char('j') | Key::ArrowDown => self.move_selection_down(),
            Key::Char('k') | Key::ArrowUp => self.move_selection_up(),
            _ => EventResult::Ignored,
        }
    }

    /// Move selection down, skipping non-selectable lines
    fn move_selection_down(&mut self) -> EventResult {
        let len = if !self.selectable_lines.is_empty() {
            self.selectable_lines.len()
        } else {
            self.left_content.len()
        };

        if len == 0 {
            return EventResult::Ignored;
        }

        let current = self.selected_line.unwrap_or(0);
        let mut next = current + 1;
        while next < len {
            if self.is_selectable(next) {
                self.selected_line = Some(next);
                if let Some(cb) = self.on_change.as_mut() {
                    let result = cb(next);
                    match result {
                        EventResult::Consumed | EventResult::Ignored => {}
                        _ => return result,
                    }
                }
                if let Some(cb) = self.on_down.as_mut() {
                    return cb(next);
                }
                return EventResult::Consumed;
            }
            next += 1;
        }
        EventResult::Consumed
    }

    /// Move selection up, skipping non-selectable lines
    fn move_selection_up(&mut self) -> EventResult {
        let len = if !self.selectable_lines.is_empty() {
            self.selectable_lines.len()
        } else {
            self.left_content.len()
        };

        if len == 0 {
            return EventResult::Ignored;
        }

        let current = self.selected_line.unwrap_or(0);
        if current == 0 {
            return EventResult::Ignored;
        }
        let mut next = current;
        while next > 0 {
            next -= 1;
            if self.is_selectable(next) {
                self.selected_line = Some(next);
                if let Some(cb) = self.on_change.as_mut() {
                    let result = cb(next);
                    match result {
                        EventResult::Consumed | EventResult::Ignored => {}
                        _ => return result,
                    }
                }
                if let Some(cb) = self.on_up.as_mut() {
                    return cb(next);
                }
                return EventResult::Consumed;
            }
        }
        EventResult::Consumed
    }

    /// Check if a line is selectable
    fn is_selectable(&self, index: usize) -> bool {
        self.selectable_lines.get(index).copied().unwrap_or(true)
    }

    /// Set selected line index
    pub fn set_selected_line(&mut self, line: Option<usize>) {
        self.selected_line = line;
    }

    /// Render the split view to a layer
    ///
    /// The split view takes up 90% of the terminal dimensions (centered)
    pub fn render(&self, layer: &mut Layer) {
        let term_rows = layer.rows();
        let term_cols = layer.cols();

        // Calculate 90% dimensions
        let width = (term_cols * 90) / 100;
        let height = (term_rows * 90) / 100;

        // Ensure minimum size
        let width = width.max(20);
        let height = height.max(5);

        let mut style = WindowStyle::new()
            .with_border(true)
            .with_reverse_video(false);

        if let Some(c) = self.fg {
            style = style.with_fg(c);
        }
        if let Some(c) = self.bg {
            style = style.with_bg(c);
        }

        let window = FloatingWindow::with_style(WindowPosition::Center, width, height, style);

        // Calculate pane widths (content area, minus border)
        let content_width = width.saturating_sub(2); // -2 for left/right borders
        let content_height = height.saturating_sub(2); // -2 for top/bottom borders

        let left_width = (content_width * self.left_width_percent as usize) / 100;
        let right_width = content_width.saturating_sub(left_width).saturating_sub(1); // -1 for divider

        // Build combined content with divider
        let mut combined_content: Vec<Vec<crate::layer::Cell>> = Vec::new();

        use crate::layer::Cell;

        for row in 0..content_height {
            let mut line: Vec<Cell> = Vec::with_capacity(content_width);

            // Left pane content
            let left_row = row + self.left_scroll;
            if let Some(left_line) = self.left_content.get(left_row) {
                for (i, cell) in left_line.iter().take(left_width).enumerate() {
                    line.push(cell.clone());
                    if i >= left_width - 1 {
                        break;
                    }
                }
                // Pad to left_width
                while line.len() < left_width {
                    line.push(Cell::new(Character::from(' ')));
                }
            } else {
                // Empty line
                line.extend(repeat_n(Cell::new(Character::from(' ')), left_width));
            }

            // Divider
            line.push(Cell::from_char(DIVIDER_CHAR));

            // Right pane content
            let right_row = row + self.right_scroll;
            if let Some(right_line) = self.right_content.get(right_row) {
                for (i, cell) in right_line.iter().take(right_width).enumerate() {
                    line.push(cell.clone());
                    if i >= right_width - 1 {
                        break;
                    }
                }
                // Pad to right_width
                while line.len() < left_width + 1 + right_width {
                    line.push(Cell::new(Character::from(' ')));
                }
            } else {
                // Empty line
                line.extend(repeat_n(Cell::new(Character::from(' ')), right_width));
            }

            combined_content.push(line);
        }

        window.render_cells(layer, &combined_content);

        // Highlight selected line in left pane
        if let Some(selected_idx) = self.selected_line {
            // Check visibility based on scroll
            if selected_idx >= self.left_scroll {
                let visual_row_idx = selected_idx - self.left_scroll;

                // Check if within visible content height
                if visual_row_idx < content_height {
                    // Calculate absolute window position
                    // SAFETY: Layer dimensions usually fit in u16 for terminals
                    let (win_row, win_col) =
                        window.calculate_position(term_rows as u16, term_cols as u16);

                    // Content starts at +1, +1 due to border
                    // Convert back to usize for layer operations
                    let abs_row = (win_row as usize) + 1 + visual_row_idx;
                    let abs_col_start = (win_col as usize) + 1;

                    // Highlight left pane width (invert colors)
                    if let Some(line) = combined_content.get(visual_row_idx) {
                        for i in 0..left_width {
                            let col = abs_col_start + i;
                            if let Some(cell) = line.get(i) {
                                layer.set_cell(
                                    abs_row,
                                    col,
                                    cell.clone().with_fg(Color::Black).with_bg(Color::White),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Default for SelectView {
    fn default() -> Self {
        Self::new()
    }
}
impl Component for SelectView {
    fn handle_input(&mut self, key: Key) -> EventResult {
        SelectView::handle_input(self, key)
    }

    fn render(&mut self, layer: &mut Layer) {
        SelectView::render(self, layer);
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
