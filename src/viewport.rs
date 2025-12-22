//! Viewport management
//! Handles the visible portion of the text buffer

/// Viewport manages which portion of the buffer is visible
pub struct Viewport {
    /// Top line of the visible area (0-indexed)
    top_line: usize,
    /// Leftmost visible column (0-indexed)
    visible_cols: usize,
    /// Number of visible rows
    visible_rows: usize,
}

impl Viewport {
    pub fn new(rows: usize, cols: usize) -> Self {
        Viewport {
            top_line: 0,
            visible_cols: cols,
            visible_rows: rows,
        }
    }

    /// Update viewport based on cursor position and total lines
    pub fn update(&mut self, cursor_line: usize, total_lines: usize) {
        // If cursor is above visible area, scroll up
        if cursor_line < self.top_line {
            self.top_line = cursor_line;
        }
        
        // If cursor is below visible area, scroll down
        let bottom_line = self.top_line + self.visible_rows.saturating_sub(1);
        if cursor_line > bottom_line && bottom_line < total_lines {
            self.top_line = cursor_line.saturating_sub(self.visible_rows.saturating_sub(1));
        }
        
        // Ensure top_line doesn't go negative (shouldn't happen with usize, but be safe)
        if self.top_line > total_lines {
            self.top_line = total_lines.saturating_sub(1).max(0);
        }
    }

    pub fn top_line(&self) -> usize {
        self.top_line
    }

    pub fn visible_rows(&self) -> usize {
        self.visible_rows
    }

    pub fn visible_cols(&self) -> usize {
        self.visible_cols
    }

    pub fn set_size(&mut self, rows: usize, cols: usize) {
        self.visible_rows = rows;
        self.visible_cols = cols;
    }
}

