//! Viewport management
//! Handles the visible portion of the text buffer

//! ## viewport/ Invariants
//!
//! - The viewport represents a window into buffer content.
//! - The viewport never mutates buffer contents.
//! - The cursor is always visible within the viewport.
//! - Viewport dimensions reflect the current terminal size.
//! - Viewport updates are explicit and predictable.
//! - Viewport logic is independent of rendering mechanics.

/// Viewport manages which portion of the buffer is visible
pub struct Viewport {
    /// Top line of the visible area (0-indexed)
    top_line: usize,
    /// Previous top line (for detecting scroll changes)
    prev_top_line: usize,
    /// Whether this is the first update (for initial render)
    first_update: bool,
    /// Leftmost visible column (0-indexed)
    visible_cols: usize,
    /// Number of visible rows
    visible_rows: usize,
}

impl Viewport {
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Viewport {
            top_line: 0,
            prev_top_line: 0,
            first_update: true,
            visible_cols: cols,
            visible_rows: rows,
        }
    }

    /// Update viewport based on cursor position and total lines
    /// Ensures the cursor is always visible by scrolling when necessary
    /// Returns true if the viewport scrolled (`top_line` changed) or if this is the first update
    pub fn update(&mut self, cursor_line: usize, total_lines: usize) -> bool {
        // Store previous top line
        self.prev_top_line = self.top_line;
        let was_first = self.first_update;
        self.first_update = false;

        // Calculate content rows (excluding status bar)
        let content_rows = self.visible_rows.saturating_sub(1);

        // Calculate the last visible content line (0-indexed)
        // If top_line = 0 and content_rows = 9, we show lines 0-8, so bottom = 8
        let bottom_content_line = self.top_line + content_rows.saturating_sub(1);

        // If cursor is above visible area, scroll up to show it
        if cursor_line < self.top_line {
            self.top_line = cursor_line;
        }

        // If cursor is below visible area, scroll down to show it
        // We want the cursor to be visible, so we position it near the bottom of the viewport
        if cursor_line > bottom_content_line {
            // Position cursor so it's visible - put it on the last content line
            // This means: top_line = cursor_line - (content_rows - 1)
            // So if cursor_line = 10 and content_rows = 9, top_line = 10 - 8 = 2
            // Then we show lines 2-10, with cursor on line 10 (last visible)
            let new_top = cursor_line.saturating_sub(content_rows.saturating_sub(1));
            self.top_line = new_top;
        }

        // Ensure we don't scroll past the end of the buffer
        // If total_lines is less than content_rows, start at 0
        if total_lines > 0 && total_lines <= content_rows {
            self.top_line = 0;
        } else if self.top_line + content_rows > total_lines && total_lines > content_rows {
            // If we're showing past the end, scroll back
            self.top_line = total_lines.saturating_sub(content_rows);
        }

        // Ensure top_line doesn't go negative (shouldn't happen with usize, but be safe)
        if self.top_line > total_lines.saturating_sub(1) && total_lines > 0 {
            self.top_line = total_lines.saturating_sub(1).max(0);
        }

        // Return true if viewport scrolled or if this is the first update
        self.top_line != self.prev_top_line || was_first
    }

    /// Get the previous top line (before last update)
    #[must_use]
    pub fn prev_top_line(&self) -> usize {
        self.prev_top_line
    }

    #[must_use]
    pub fn top_line(&self) -> usize {
        self.top_line
    }

    #[must_use]
    pub fn visible_rows(&self) -> usize {
        self.visible_rows
    }

    #[must_use]
    pub fn visible_cols(&self) -> usize {
        self.visible_cols
    }

    pub fn set_size(&mut self, rows: usize, cols: usize) {
        self.visible_rows = rows;
        self.visible_cols = cols;
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
