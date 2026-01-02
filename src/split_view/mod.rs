//! Reusable split-view overlay component
//!
//! A 90% width/height floating window with left and right panes
//! separated by a vertical divider.

use crate::color::Color;
use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
use crate::layer::Layer;

/// Vertical divider character
const DIVIDER_CHAR: char = '│';

/// A split-view overlay that renders left and right panes
#[derive(Debug, Clone)]
pub struct SplitView {
    /// Percentage of width allocated to left pane (0-100)
    left_width_percent: u8,
    /// Content for the left pane
    left_content: Vec<Vec<char>>,
    /// Content for the right pane
    right_content: Vec<Vec<char>>,
    /// Scroll offset for left pane
    left_scroll: usize,
    /// Scroll offset for right pane
    right_scroll: usize,
}

impl SplitView {
    /// Create a new split view with 40% left pane
    pub fn new() -> Self {
        Self {
            left_width_percent: 40,
            left_content: Vec::new(),
            right_content: Vec::new(),
            left_scroll: 0,
            right_scroll: 0,
        }
    }

    /// Set the left pane width percentage
    #[must_use]
    pub fn with_left_width(mut self, percent: u8) -> Self {
        self.left_width_percent = percent.min(90).max(10);
        self
    }

    /// Set the left pane content
    pub fn set_left_content(&mut self, content: Vec<Vec<char>>) {
        self.left_content = content;
    }

    /// Set the right pane content
    pub fn set_right_content(&mut self, content: Vec<Vec<char>>) {
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

        // Create the floating window
        let style = WindowStyle::new()
            .with_border(true)
            .with_reverse_video(false)
            .with_fg(Color::White)
            .with_bg(Color::Black);

        let window = FloatingWindow::with_style(WindowPosition::Center, width, height, style);

        // Calculate pane widths (content area, minus border)
        let content_width = width.saturating_sub(2); // -2 for left/right borders
        let content_height = height.saturating_sub(2); // -2 for top/bottom borders

        let left_width = (content_width * self.left_width_percent as usize) / 100;
        let right_width = content_width.saturating_sub(left_width).saturating_sub(1); // -1 for divider

        // Build combined content with divider
        let mut combined_content: Vec<Vec<char>> = Vec::new();

        for row in 0..content_height {
            let mut line: Vec<char> = Vec::with_capacity(content_width);

            // Left pane content
            let left_row = row + self.left_scroll;
            if let Some(left_line) = self.left_content.get(left_row) {
                for (i, &byte) in left_line.iter().take(left_width).enumerate() {
                    line.push(byte);
                    if i >= left_width - 1 {
                        break;
                    }
                }
                // Pad to left_width
                while line.len() < left_width {
                    line.push(' ');
                }
            } else {
                // Empty line
                line.extend(std::iter::repeat(' ').take(left_width));
            }

            // Divider
            line.push(DIVIDER_CHAR);

            // Right pane content
            let right_row = row + self.right_scroll;
            if let Some(right_line) = self.right_content.get(right_row) {
                for (i, &byte) in right_line.iter().take(right_width).enumerate() {
                    line.push(byte);
                    if i >= right_width - 1 {
                        break;
                    }
                }
                // Pad to right_width
                while line.len() < left_width + 1 + right_width {
                    line.push(' ');
                }
            } else {
                // Empty line
                line.extend(std::iter::repeat(' ').take(right_width));
            }

            combined_content.push(line);
        }

        // Render using FloatingWindow
        window.render(layer, &combined_content);
    }
}

impl Default for SplitView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
