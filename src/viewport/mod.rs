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
    /// Top visual row when soft-wrap is active (0-indexed)
    top_visual_row: usize,
    /// Previous top visual row (for detecting scroll changes in wrap mode)
    prev_top_visual_row: usize,
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
    /// Fractional scroll offset in [0.0, 1.0) below top_line; no current
    /// caller reads this, and top_line() stays integer-only.
    sub_line_offset: f64,
}

impl Viewport {
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Viewport {
            top_line: 0,
            prev_top_line: 0,
            top_visual_row: 0,
            prev_top_visual_row: 0,
            first_update: true,
            visible_cols: cols,
            left_col: 0,
            prev_left_col: 0,
            visible_rows: rows,
            sub_line_offset: 0.0,
        }
    }

    /// Update viewport based on cursor position and total lines.
    /// Keeps the cursor vertically centered whenever the document is large enough;
    /// clamps naturally at the top and bottom edges.
    /// Returns true if a full terminal redraw is required (first update after a
    /// reset); plain scrolls repaint through the double-buffer cell diff.
    pub fn update(
        &mut self,
        cursor_line: usize,
        cursor_col: usize,
        total_lines: usize,
        gutter_width: usize,
    ) -> bool {
        self.prev_top_line = self.top_line;
        self.prev_left_col = self.left_col;
        let was_first = self.first_update;
        self.first_update = false;

        // --- Vertical: keep cursor centered ---
        let content_rows = self.visible_rows.saturating_sub(1);
        let half = content_rows / 2;
        let ideal_top = cursor_line.saturating_sub(half);
        let max_top = total_lines.saturating_sub(content_rows);
        self.top_line = ideal_top.min(max_top);
        self.sub_line_offset = 0.0;

        // --- Horizontal: scroll just enough to keep cursor visible ---
        let content_width = self.visible_cols.saturating_sub(gutter_width);
        if content_width > 0 {
            if cursor_col < self.left_col {
                self.left_col = cursor_col;
            } else if cursor_col >= self.left_col + content_width {
                self.left_col = cursor_col.saturating_sub(content_width.saturating_sub(1));
            }
        }

        was_first
    }

    /// Soft-wrap variant of [`Self::update`]; same full-redraw return contract.
    pub fn update_visual(
        &mut self,
        cursor_visual_row: usize,
        _cursor_visual_col: usize,
        total_visual_rows: usize,
        _gutter_width: usize,
    ) -> bool {
        self.prev_top_visual_row = self.top_visual_row;
        let was_first = self.first_update;
        self.first_update = false;

        let content_rows = self.visible_rows.saturating_sub(1);
        let half = content_rows / 2;
        let ideal_top = cursor_visual_row.saturating_sub(half);
        let max_top = total_visual_rows.saturating_sub(content_rows);
        self.top_visual_row = ideal_top.min(max_top);
        self.sub_line_offset = 0.0;

        self.left_col = 0;
        self.prev_left_col = 0;

        was_first
    }

    /// Get the previous top line (before last update)
    #[must_use]
    pub fn prev_top_line(&self) -> usize {
        self.prev_top_line
    }

    /// Get the previous top visual row (before last update)
    #[must_use]
    pub fn prev_top_visual_row(&self) -> usize {
        self.prev_top_visual_row
    }

    /// Get the previous leftmost visible column (before last update)
    #[must_use]
    pub fn prev_left_col(&self) -> usize {
        self.prev_left_col
    }

    #[must_use]
    pub fn top_line(&self) -> usize {
        self.top_line
    }

    #[must_use]
    pub fn top_visual_row(&self) -> usize {
        self.top_visual_row
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
        self.sub_line_offset = 0.0;
        // Mark as needing update to ensure proper rendering
        self.first_update = true;
    }

    /// Scroll so that `line` (0-indexed) is vertically centered in the viewport.
    /// Clamps correctly so no blank space appears below the buffer.
    pub fn center_on(&mut self, line: usize, total_lines: usize) {
        let content_rows = self.visible_rows.saturating_sub(1);
        let half = content_rows / 2;
        let ideal_top = line.saturating_sub(half);
        let max_top = total_lines.saturating_sub(content_rows);
        self.top_line = ideal_top.min(max_top);
        self.sub_line_offset = 0.0;
        self.first_update = true;
    }

    /// Get current scroll position as (top_line, left_col)
    pub fn get_scroll(&self) -> (usize, usize) {
        (self.top_line, self.left_col)
    }

    /// Fractional scroll offset in [0.0, 1.0] below `top_line`. Always 0.0
    /// until a future renderer calls `set_sub_line_offset`.
    #[must_use]
    pub fn sub_line_offset(&self) -> f64 {
        self.sub_line_offset
    }

    /// Set the fractional scroll offset below `top_line`, clamped to
    /// [0.0, 1.0]. Does not itself move `top_line`.
    pub fn set_sub_line_offset(&mut self, offset: f64) {
        self.sub_line_offset = offset.clamp(0.0, 1.0);
    }

    pub fn mark_needs_full_redraw(&mut self) {
        self.first_update = true;
        self.prev_top_visual_row = self.top_visual_row;
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
