//! Floating window component
//! Reusable overlay window that can be rendered on top of existing content
//!
//! ## floating_window/ Invariants
//!
//! - Floating windows never mutate editor or buffer state.
//! - Floating windows are positioned relative to terminal coordinates.
//! - Window content is provided externally and rendered as-is.
//! - Window rendering does not affect underlying content (caller must restore).
//! - Window dimensions are constrained to terminal size.
//! - Window position is validated to ensure it fits within terminal bounds.

use crate::term::TerminalBackend;

// ANSI escape sequences
const REVERSE_VIDEO_ON: &[u8] = b"\x1b[7m";
const RESET: &[u8] = b"\x1b[0m";

// Border characters
const BORDER_TOP_LEFT: &[u8] = b"+";
const BORDER_TOP_RIGHT: &[u8] = b"+";
const BORDER_BOTTOM_LEFT: &[u8] = b"+";
const BORDER_BOTTOM_RIGHT: &[u8] = b"+";
const BORDER_HORIZONTAL: &[u8] = b"-";
const BORDER_VERTICAL: &[u8] = b"|";

/// Position for floating window
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPosition {
    /// Center the window horizontally and vertically
    Center,
    /// Position at specific row and column (0-indexed)
    Absolute { row: u16, col: u16 },
    /// Position at bottom of screen, centered horizontally
    Bottom,
    /// Position at top of screen, centered horizontally
    Top,
}

/// Floating window configuration
#[derive(Debug, Clone)]
pub struct FloatingWindow {
    /// Window position
    position: WindowPosition,
    /// Window width in columns
    width: usize,
    /// Window height in rows
    height: usize,
    /// Whether to draw a border around the window
    border: bool,
    /// Whether to use reverse video (inverted colors) for the window
    reverse_video: bool,
}

impl FloatingWindow {
    /// Create a new floating window
    pub fn new(position: WindowPosition, width: usize, height: usize) -> Self {
        FloatingWindow {
            position,
            width,
            height,
            border: true,
            reverse_video: true,
        }
    }

    /// Set whether to draw a border
    pub fn with_border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }

    /// Set whether to use reverse video
    pub fn with_reverse_video(mut self, reverse: bool) -> Self {
        self.reverse_video = reverse;
        self
    }

    /// Calculate the actual position of the window given terminal dimensions
    /// Returns (row, col) where the window should be positioned
    pub fn calculate_position(&self, term_rows: u16, term_cols: u16) -> (u16, u16) {
        let width = self.width.min(term_cols as usize) as u16;
        let height = self.height.min(term_rows as usize) as u16;

        match self.position {
            WindowPosition::Center => {
                let row = (term_rows.saturating_sub(height)) / 2;
                let col = (term_cols.saturating_sub(width)) / 2;
                (row, col)
            }
            WindowPosition::Absolute { row, col } => {
                // Clamp to terminal bounds
                let row = row.min(term_rows.saturating_sub(height));
                let col = col.min(term_cols.saturating_sub(width));
                (row, col)
            }
            WindowPosition::Bottom => {
                let row = term_rows.saturating_sub(height);
                let col = (term_cols.saturating_sub(width)) / 2;
                (row, col)
            }
            WindowPosition::Top => {
                let row = 0;
                let col = (term_cols.saturating_sub(width)) / 2;
                (row, col)
            }
        }
    }

    /// Render a single line of content with padding
    /// If `move_cursor` is true, moves cursor to (row, col) first
    fn render_content_line<T: TerminalBackend>(
        term: &mut T,
        row: u16,
        col: u16,
        line: Option<&Vec<u8>>,
        width: usize,
        move_cursor: bool,
    ) -> Result<(), String> {
        if move_cursor {
            term.move_cursor(row, col)?;
        }
        
        if let Some(line) = line {
            // Truncate line to fit
            let display_line: Vec<u8> = line.iter()
                .take(width)
                .copied()
                .collect();
            
            // Write content
            term.write(&display_line)?;
            
            // Pad with spaces if needed
            let padding = width.saturating_sub(display_line.len());
            for _ in 0..padding {
                term.write(b" ")?;
            }
        } else {
            // Empty line - fill with spaces
            for _ in 0..width {
                term.write(b" ")?;
            }
        }
        
        Ok(())
    }

    /// Render the top border of the window
    fn render_top_border<T: TerminalBackend>(
        term: &mut T,
        row: u16,
        col: u16,
        width: usize,
    ) -> Result<(), String> {
        term.move_cursor(row, col)?;
        term.write(BORDER_TOP_LEFT)?;
        for _ in 0..width.saturating_sub(2) {
            term.write(BORDER_HORIZONTAL)?;
        }
        if width > 1 {
            term.write(BORDER_TOP_RIGHT)?;
        }
        Ok(())
    }

    /// Render the bottom border of the window
    fn render_bottom_border<T: TerminalBackend>(
        term: &mut T,
        row: u16,
        col: u16,
        width: usize,
    ) -> Result<(), String> {
        term.move_cursor(row, col)?;
        term.write(BORDER_BOTTOM_LEFT)?;
        for _ in 0..width.saturating_sub(2) {
            term.write(BORDER_HORIZONTAL)?;
        }
        if width > 1 {
            term.write(BORDER_BOTTOM_RIGHT)?;
        }
        Ok(())
    }

    /// Render the floating window with content
    /// 
    /// `content` is a vector of lines, where each line is a byte vector.
    /// Lines will be truncated to fit within the window width.
    /// If there are more lines than the window height, they will be truncated.
    pub fn render<T: TerminalBackend>(
        &self,
        term: &mut T,
        content: &[Vec<u8>],
    ) -> Result<(), String> {
        // Get terminal size
        let size = term.get_size()?;
        let term_rows = size.rows;
        let term_cols = size.cols;

        // Calculate actual position
        let (start_row, start_col) = self.calculate_position(term_rows, term_cols);
        
        // Clamp dimensions to terminal size
        let width = self.width.min(term_cols as usize);
        let height = self.height.min(term_rows as usize);

        // Apply reverse video if enabled
        if self.reverse_video {
            term.write(REVERSE_VIDEO_ON)?;
        }

        // Render border and content
        if self.border {
            // Top border
            Self::render_top_border(term, start_row, start_col, width)?;
            
            // Content rows with side borders
            let content_height = height.saturating_sub(2); // Subtract top and bottom borders
            let content_width = width.saturating_sub(2);
            
            for content_row in 0..content_height {
                let row = start_row + 1 + content_row as u16;
                term.move_cursor(row, start_col)?;
                term.write(BORDER_VERTICAL)?;
                
                // Render content line (cursor already positioned after left border)
                let line = content.get(content_row);
                Self::render_content_line(term, row, start_col + 1, line, content_width, false)?;
                
                // Right border
                if width > 1 {
                    term.move_cursor(row, start_col + content_width as u16 + 1)?;
                    term.write(BORDER_VERTICAL)?;
                }
            }
            
            // Bottom border
            if height > 1 {
                let bottom_row = start_row + height as u16 - 1;
                Self::render_bottom_border(term, bottom_row, start_col, width)?;
            }
        } else {
            // No border - just render content
            for row_offset in 0..height {
                let row = start_row + row_offset as u16;
                let line = content.get(row_offset);
                Self::render_content_line(term, row, start_col, line, width, true)?;
            }
        }

        // Reset colors
        if self.reverse_video {
            term.write(RESET)?;
        }

        Ok(())
    }

    /// Render a single-line floating window (useful for command line)
    /// 
    /// `prompt` is displayed at the start, followed by `content`
    pub fn render_single_line<T: TerminalBackend>(
        &self,
        term: &mut T,
        prompt: &str,
        content: &str,
    ) -> Result<(), String> {
        // Combine prompt and content
        let mut line = Vec::new();
        line.extend_from_slice(prompt.as_bytes());
        line.extend_from_slice(content.as_bytes());
        
        self.render(term, &[line])
    }

    /// Get the width of the window
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the height of the window
    pub fn height(&self) -> usize {
        self.height
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

