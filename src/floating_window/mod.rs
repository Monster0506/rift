//! Floating window component
//! Reusable overlay window that can be rendered on top of existing content
//!
//! ## `floating_window`/ Invariants
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

// Default border characters
const DEFAULT_BORDER_TOP_LEFT: &[u8] = "╭".as_bytes();
const DEFAULT_BORDER_TOP_RIGHT: &[u8] = "╮".as_bytes();
const DEFAULT_BORDER_BOTTOM_LEFT: &[u8] = "╰".as_bytes();
const DEFAULT_BORDER_BOTTOM_RIGHT: &[u8] = "╯".as_bytes();
const DEFAULT_BORDER_HORIZONTAL: &[u8] = "─".as_bytes();
const DEFAULT_BORDER_VERTICAL: &[u8] = "│".as_bytes();

/// Border characters for floating windows
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorderChars {
    /// Top-left corner character
    pub top_left: Vec<u8>,
    /// Top-right corner character
    pub top_right: Vec<u8>,
    /// Bottom-left corner character
    pub bottom_left: Vec<u8>,
    /// Bottom-right corner character
    pub bottom_right: Vec<u8>,
    /// Horizontal line character
    pub horizontal: Vec<u8>,
    /// Vertical line character
    pub vertical: Vec<u8>,
}

impl BorderChars {
    /// Create default border characters (Unicode box drawing)
    #[must_use]
    pub fn default() -> Self {
        BorderChars {
            top_left: DEFAULT_BORDER_TOP_LEFT.to_vec(),
            top_right: DEFAULT_BORDER_TOP_RIGHT.to_vec(),
            bottom_left: DEFAULT_BORDER_BOTTOM_LEFT.to_vec(),
            bottom_right: DEFAULT_BORDER_BOTTOM_RIGHT.to_vec(),
            horizontal: DEFAULT_BORDER_HORIZONTAL.to_vec(),
            vertical: DEFAULT_BORDER_VERTICAL.to_vec(),
        }
    }

    /// Create border characters from byte slices
    #[must_use]
    pub fn new(
        top_left: &[u8],
        top_right: &[u8],
        bottom_left: &[u8],
        bottom_right: &[u8],
        horizontal: &[u8],
        vertical: &[u8],
    ) -> Self {
        BorderChars {
            top_left: top_left.to_vec(),
            top_right: top_right.to_vec(),
            bottom_left: bottom_left.to_vec(),
            bottom_right: bottom_right.to_vec(),
            horizontal: horizontal.to_vec(),
            vertical: vertical.to_vec(),
        }
    }

    /// Create border characters from single-byte ASCII characters
    #[must_use]
    pub fn from_ascii(
        top_left: u8,
        top_right: u8,
        bottom_left: u8,
        bottom_right: u8,
        horizontal: u8,
        vertical: u8,
    ) -> Self {
        BorderChars {
            top_left: vec![top_left],
            top_right: vec![top_right],
            bottom_left: vec![bottom_left],
            bottom_right: vec![bottom_right],
            horizontal: vec![horizontal],
            vertical: vec![vertical],
        }
    }
}

impl Default for BorderChars {
    fn default() -> Self {
        BorderChars::default()
    }
}

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
    /// Custom border characters (None uses defaults)
    border_chars: Option<BorderChars>,
}

impl FloatingWindow {
    /// Create a new floating window
    #[must_use]
    pub fn new(position: WindowPosition, width: usize, height: usize) -> Self {
        FloatingWindow {
            position,
            width,
            height,
            border: true,
            reverse_video: true,
            border_chars: None,
        }
    }

    /// Set whether to draw a border
    #[must_use]
    pub fn with_border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }

    /// Set whether to use reverse video
    #[must_use]
    pub fn with_reverse_video(mut self, reverse: bool) -> Self {
        self.reverse_video = reverse;
        self
    }

    /// Set custom border characters
    #[must_use]
    pub fn with_border_chars(mut self, border_chars: BorderChars) -> Self {
        self.border_chars = Some(border_chars);
        self
    }

    /// Calculate the actual position of the window given terminal dimensions
    /// Returns (row, col) where the window should be positioned
    #[must_use]
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

    /// Write ANSI cursor positioning escape sequence to buffer
    /// Row and col are 1-indexed (ANSI standard)
    fn write_cursor_position(buf: &mut Vec<u8>, row: u16, col: u16) {
        buf.push(0x1b); // ESC
        buf.push(b'[');

        // Convert row to decimal string
        let mut row_digits = Vec::new();
        let mut r = row;
        if r == 0 {
            row_digits.push(b'1'); // ANSI is 1-indexed, 0 means row 1
        } else {
            while r > 0 {
                row_digits.push(b'0' + (r % 10) as u8);
                r /= 10;
            }
            row_digits.reverse();
        }
        buf.extend_from_slice(&row_digits);

        buf.push(b';');

        // Convert col to decimal string
        let mut col_digits = Vec::new();
        let mut c = col;
        if c == 0 {
            col_digits.push(b'1'); // ANSI is 1-indexed, 0 means col 1
        } else {
            while c > 0 {
                col_digits.push(b'0' + (c % 10) as u8);
                c /= 10;
            }
            col_digits.reverse();
        }
        buf.extend_from_slice(&col_digits);

        buf.push(b'H');
    }

    /// Render the floating window with content
    ///
    /// `content` is a vector of lines, where each line is a byte vector.
    /// Lines will be truncated to fit within the window width.
    /// If there are more lines than the window height, they will be truncated.
    ///
    /// `border_chars_override` allows overriding border characters for this render call.
    /// If None, uses the window's configured `border_chars` or defaults.
    ///
    /// This method batches all writes to minimize flicker by building the entire
    /// window in memory before writing it all at once.
    pub fn render<T: TerminalBackend>(
        &self,
        term: &mut T,
        content: &[Vec<u8>],
    ) -> Result<(), String> {
        self.render_with_border_chars(term, content, None)
    }

    /// Render the floating window with content and optional border character override
    ///
    /// `border_chars_override` allows overriding border characters for this render call.
    /// If Some, uses those characters. If None, uses the window's configured `border_chars` or defaults.
    pub fn render_with_border_chars<T: TerminalBackend>(
        &self,
        term: &mut T,
        content: &[Vec<u8>],
        border_chars_override: Option<BorderChars>,
    ) -> Result<(), String> {
        // Determine which border chars to use: override > window config > defaults
        let border_chars = border_chars_override
            .or(self.border_chars.clone())
            .unwrap_or_else(BorderChars::default);
        // Get terminal size
        let size = term.get_size()?;
        let term_rows = size.rows;
        let term_cols = size.cols;

        // Calculate actual position
        let (start_row, start_col) = self.calculate_position(term_rows, term_cols);

        // Clamp dimensions to terminal size
        let width = self.width.min(term_cols as usize);
        let height = self.height.min(term_rows as usize);

        // Build entire window in memory to minimize writes and reduce flicker
        let mut output = Vec::new();

        // Apply reverse video if enabled
        if self.reverse_video {
            output.extend_from_slice(REVERSE_VIDEO_ON);
        }

        // Render border and content
        if self.border {
            let content_height = height.saturating_sub(2); // Subtract top and bottom borders
            let content_width = width.saturating_sub(2);

            // Top border: +----+
            // ANSI positions are 1-indexed, so add 1 to row/col
            Self::write_cursor_position(&mut output, start_row + 1, start_col + 1);
            output.extend_from_slice(&border_chars.top_left);
            // Repeat horizontal character for content width
            // Note: This assumes horizontal is a single display character (may be multi-byte)
            for _ in 0..content_width {
                output.extend_from_slice(&border_chars.horizontal);
            }
            if width > 1 {
                output.extend_from_slice(&border_chars.top_right);
            }

            // Content rows with side borders: |content|
            for content_row in 0..content_height {
                let row = start_row + 1 + content_row as u16;
                Self::write_cursor_position(&mut output, row + 1, start_col + 1);
                output.extend_from_slice(&border_chars.vertical);

                // Content
                let line = content.get(content_row);
                if let Some(line) = line {
                    // Truncate line to fit
                    let display_line: Vec<u8> = line.iter().take(content_width).copied().collect();
                    output.extend_from_slice(&display_line);

                    // Pad with spaces if needed
                    let padding = content_width.saturating_sub(display_line.len());
                    output.extend(std::iter::repeat_n(b' ', padding));
                } else {
                    // Empty line - fill with spaces
                    output.extend(std::iter::repeat_n(b' ', content_width));
                }

                // Right border
                if width > 1 {
                    output.extend_from_slice(&border_chars.vertical);
                }
            }

            // Bottom border: +----+
            if height > 1 {
                let bottom_row = start_row + height as u16;
                Self::write_cursor_position(&mut output, bottom_row, start_col + 1);
                output.extend_from_slice(&border_chars.bottom_left);
                // Repeat horizontal character for content width
                for _ in 0..content_width {
                    output.extend_from_slice(&border_chars.horizontal);
                }
                if width > 1 {
                    output.extend_from_slice(&border_chars.bottom_right);
                }
            }
        } else {
            // No border - just render content
            for row_offset in 0..height {
                let row = start_row + row_offset as u16;
                Self::write_cursor_position(&mut output, row + 1, start_col + 1);

                let line = content.get(row_offset);
                if let Some(line) = line {
                    // Truncate line to fit
                    let display_line: Vec<u8> = line.iter().take(width).copied().collect();
                    output.extend_from_slice(&display_line);

                    // Pad with spaces if needed
                    let padding = width.saturating_sub(display_line.len());
                    output.extend(std::iter::repeat_n(b' ', padding));
                } else {
                    // Empty line - fill with spaces
                    output.extend(std::iter::repeat_n(b' ', width));
                }
            }
        }

        // Reset colors
        if self.reverse_video {
            output.extend_from_slice(RESET);
        }

        // Write everything at once
        term.write(&output)?;

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
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the height of the window
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
