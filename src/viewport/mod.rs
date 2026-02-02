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
#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    /// Top line of the visible area (0-indexed)
    top_line: usize,
    /// Previous top line (for detecting scroll changes)
    prev_top_line: usize,
    /// Whether this is the first update (for initial render)
    first_update: bool,
    /// Leftmost visible column (0-indexed)
    left_col: usize,
    /// Previous left column (for detecting scroll changes)
    prev_left_col: usize,
    /// Number of visible rows
    visible_rows: usize,
    /// Number of visible columns
    visible_cols: usize,
}

impl Viewport {
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Viewport {
            top_line: 0,
            prev_top_line: 0,
            first_update: true,
            visible_cols: cols,
            left_col: 0,
            prev_left_col: 0,
            visible_rows: rows,
        }
    }

    /// Update viewport based on cursor position and total lines
    /// Ensures the cursor is always visible by scrolling when necessary
    /// Returns true if the viewport scrolled or if this is the first update
    pub fn update(
        &mut self,
        cursor_line: usize,
        cursor_col: usize,
        total_lines: usize,
        gutter_width: usize,
    ) -> bool {
        // Store previous positions
        self.prev_top_line = self.top_line;
        self.prev_left_col = self.left_col;
        let was_first = self.first_update;
        self.first_update = false;

        // --- Vertical Scrolling ---

        // Calculate content rows (excluding status bar)
        let content_rows = self.visible_rows.saturating_sub(1);

        // Calculate the last visible content line (0-indexed)
        let bottom_content_line = self.top_line + content_rows.saturating_sub(1);

        // If cursor is above visible area, scroll up
        if cursor_line < self.top_line {
            self.top_line = cursor_line;
        }

        // If cursor is below visible area, scroll down
        if cursor_line > bottom_content_line {
            let new_top = cursor_line.saturating_sub(content_rows.saturating_sub(1));
            self.top_line = new_top;
        }

        // Ensure we don't scroll past the end of the buffer
        if total_lines > 0 && total_lines <= content_rows {
            self.top_line = 0;
        } else if self.top_line + content_rows > total_lines && total_lines > content_rows {
            self.top_line = total_lines.saturating_sub(content_rows);
        }

        // Ensure top_line doesn't go negative
        if self.top_line > total_lines.saturating_sub(1) && total_lines > 0 {
            self.top_line = total_lines.saturating_sub(1).max(0);
        }

        // --- Horizontal Scrolling ---

        // Effective visible width depends on gutter
        let content_width = self.visible_cols.saturating_sub(gutter_width);

        // If content width is 0 (terminal too small), we can't do much
        if content_width > 0 {
            let right_limit = self.left_col + content_width.saturating_sub(1);

            // If cursor is to the left of visible area, scroll left
            if cursor_col < self.left_col {
                self.left_col = cursor_col;
            }

            // If cursor is to the right of visible area, scroll right
            if cursor_col > right_limit {
                // Position cursor at right edge
                self.left_col = cursor_col.saturating_sub(content_width.saturating_sub(1));
            }
        }

        // Return true if viewport scrolled or if this is the first update
        self.top_line != self.prev_top_line || self.left_col != self.prev_left_col || was_first
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

    /// Get the leftmost visible column
    #[must_use]
    pub fn left_col(&self) -> usize {
        self.left_col
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

    /// Set the scroll position (used when restoring view state)
    pub fn set_scroll(&mut self, top_line: usize, left_col: usize) {
        self.top_line = top_line;
        self.left_col = left_col;
        // Mark as needing update to ensure proper rendering
        self.first_update = true;
    }

    /// Get current scroll position as (top_line, left_col)
    pub fn get_scroll(&self) -> (usize, usize) {
        (self.top_line, self.left_col)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
