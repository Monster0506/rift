//! Layer compositing system
//! Manages rendering of UI components at different z-levels
//!
//! ## layer/ Invariants
//!
//! - Layers are rendered in priority order (lowest first, highest on top).
//! - Each layer renders independently without knowledge of other layers.
//! - Transparent cells (None) show through to lower layers.
//! - The compositor manages all layer creation and compositing.
//! - Layer modifications only affect that layer's buffer.

use crate::color::Color;
use std::collections::BTreeMap;

/// Layer priority - higher values render on top
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayerPriority(pub u8);

impl LayerPriority {
    /// Main text buffer content
    pub const CONTENT: LayerPriority = LayerPriority(0);
    /// Status line at the bottom of the screen
    pub const STATUS_BAR: LayerPriority = LayerPriority(10);
    /// Floating windows like command line, dialogs
    pub const FLOATING_WINDOW: LayerPriority = LayerPriority(20);
    /// Popup menus like autocomplete
    pub const POPUP: LayerPriority = LayerPriority(30);
    /// Hover information
    pub const HOVER: LayerPriority = LayerPriority(40);
    /// Tooltips and hints
    pub const TOOLTIP: LayerPriority = LayerPriority(50);
}

/// A cell in the terminal buffer
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// The character to display (UTF-8 encoded bytes)
    pub content: Vec<u8>,
    /// Foreground color (None = default)
    pub fg: Option<Color>,
    /// Background color (None = default)
    pub bg: Option<Color>,
}

impl Cell {
    /// Create a new cell with the given character
    pub fn new(ch: u8) -> Self {
        Self {
            content: vec![ch],
            fg: None,
            bg: None,
        }
    }

    /// Create a new cell with UTF-8 content
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            content: bytes.to_vec(),
            fg: None,
            bg: None,
        }
    }

    /// Create an empty (space) cell
    pub fn empty() -> Self {
        Self::new(b' ')
    }

    /// Set foreground color
    pub fn with_fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }

    /// Set background color
    pub fn with_bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }

    /// Set both foreground and background colors
    pub fn with_colors(mut self, fg: Option<Color>, bg: Option<Color>) -> Self {
        self.fg = fg;
        self.bg = bg;
        self
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::empty()
    }
}

/// A single rendering layer
/// Each layer contains a grid of optional cells.
/// None means transparent (shows through to lower layer).
#[derive(Debug, Clone)]
pub struct Layer {
    /// The priority (z-order) of this layer
    priority: LayerPriority,
    /// Grid of cells - outer vec is rows, inner vec is columns
    /// None = transparent, Some = content
    cells: Vec<Vec<Option<Cell>>>,
    /// Whether this layer has been modified since last composite
    dirty: bool,
    /// Number of rows in the layer
    rows: usize,
    /// Number of columns in the layer
    cols: usize,
}

impl Layer {
    /// Create a new layer with the given dimensions
    pub fn new(priority: LayerPriority, rows: usize, cols: usize) -> Self {
        let cells = vec![vec![None; cols]; rows];
        Self {
            priority,
            cells,
            dirty: true,
            rows,
            cols,
        }
    }

    /// Get the layer's priority
    pub fn priority(&self) -> LayerPriority {
        self.priority
    }

    /// Check if the layer has been modified
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the layer as clean (after compositing)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Clear all cells in the layer (make transparent)
    pub fn clear(&mut self) {
        for row in &mut self.cells {
            for cell in row.iter_mut() {
                *cell = None;
            }
        }
        self.dirty = true;
    }

    /// Set a cell at the given position
    /// Returns false if position is out of bounds
    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) -> bool {
        if row < self.rows && col < self.cols {
            self.cells[row][col] = Some(cell);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Set a cell to transparent at the given position
    pub fn clear_cell(&mut self, row: usize, col: usize) -> bool {
        if row < self.rows && col < self.cols {
            self.cells[row][col] = None;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Get a cell at the given position
    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        if row < self.rows && col < self.cols {
            self.cells[row][col].as_ref()
        } else {
            None
        }
    }

    /// Write a string of bytes at the given position
    /// Each byte becomes a separate cell
    pub fn write_bytes(&mut self, row: usize, start_col: usize, bytes: &[u8]) {
        for (i, &byte) in bytes.iter().enumerate() {
            let col = start_col + i;
            if col < self.cols {
                self.set_cell(row, col, Cell::new(byte));
            }
        }
    }

    /// Write a string of bytes with colors at the given position
    pub fn write_bytes_colored(
        &mut self,
        row: usize,
        start_col: usize,
        bytes: &[u8],
        fg: Option<Color>,
        bg: Option<Color>,
    ) {
        for (i, &byte) in bytes.iter().enumerate() {
            let col = start_col + i;
            if col < self.cols {
                self.set_cell(row, col, Cell::new(byte).with_colors(fg, bg));
            }
        }
    }

    /// Write UTF-8 content at the given position
    /// Handles multi-byte characters by putting them in a single cell
    pub fn write_utf8(&mut self, row: usize, col: usize, content: &[u8]) -> bool {
        self.set_cell(row, col, Cell::from_bytes(content))
    }

    /// Fill a row with a character
    pub fn fill_row(&mut self, row: usize, ch: u8, fg: Option<Color>, bg: Option<Color>) {
        if row < self.rows {
            for col in 0..self.cols {
                self.set_cell(row, col, Cell::new(ch).with_colors(fg, bg));
            }
        }
    }

    /// Fill a rectangular region with a character
    pub fn fill_rect(
        &mut self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
        ch: u8,
        fg: Option<Color>,
        bg: Option<Color>,
    ) {
        for row in start_row..=end_row.min(self.rows.saturating_sub(1)) {
            for col in start_col..=end_col.min(self.cols.saturating_sub(1)) {
                self.set_cell(row, col, Cell::new(ch).with_colors(fg, bg));
            }
        }
    }

    /// Resize the layer to new dimensions
    /// Content is preserved where possible
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        let mut new_cells = vec![vec![None; new_cols]; new_rows];

        // Copy existing content
        for (r, row) in self.cells.iter().enumerate() {
            if r >= new_rows {
                break;
            }
            for (c, cell) in row.iter().enumerate() {
                if c >= new_cols {
                    break;
                }
                new_cells[r][c] = cell.clone();
            }
        }

        self.cells = new_cells;
        self.rows = new_rows;
        self.cols = new_cols;
        self.dirty = true;
    }

    /// Get the number of rows
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Get the number of columns
    pub fn cols(&self) -> usize {
        self.cols
    }
}

/// Layer compositor that manages multiple layers and composites them for rendering
pub struct LayerCompositor {
    /// Layers indexed by priority
    layers: BTreeMap<LayerPriority, Layer>,
    /// Terminal dimensions
    rows: usize,
    cols: usize,
    /// Composited output buffer
    composited: Vec<Vec<Cell>>,
    /// Whether any layer is dirty and needs recompositing
    needs_composite: bool,
}

impl LayerCompositor {
    /// Create a new compositor with the given terminal dimensions
    pub fn new(rows: usize, cols: usize) -> Self {
        let composited = vec![vec![Cell::empty(); cols]; rows];
        Self {
            layers: BTreeMap::new(),
            rows,
            cols,
            composited,
            needs_composite: true,
        }
    }

    /// Get or create a layer with the given priority
    pub fn get_layer_mut(&mut self, priority: LayerPriority) -> &mut Layer {
        self.needs_composite = true;
        self.layers
            .entry(priority)
            .or_insert_with(|| Layer::new(priority, self.rows, self.cols))
    }

    /// Get a layer by priority (read-only)
    pub fn get_layer(&self, priority: LayerPriority) -> Option<&Layer> {
        self.layers.get(&priority)
    }

    /// Remove a layer
    pub fn remove_layer(&mut self, priority: LayerPriority) -> Option<Layer> {
        self.needs_composite = true;
        self.layers.remove(&priority)
    }

    /// Clear all layers
    pub fn clear_all(&mut self) {
        for layer in self.layers.values_mut() {
            layer.clear();
        }
        self.needs_composite = true;
    }

    /// Clear a specific layer
    pub fn clear_layer(&mut self, priority: LayerPriority) {
        if let Some(layer) = self.layers.get_mut(&priority) {
            layer.clear();
            self.needs_composite = true;
        }
    }

    /// Resize all layers to new dimensions
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        self.rows = new_rows;
        self.cols = new_cols;
        self.composited = vec![vec![Cell::empty(); new_cols]; new_rows];

        for layer in self.layers.values_mut() {
            layer.resize(new_rows, new_cols);
        }
        self.needs_composite = true;
    }

    /// Composite all layers into the output buffer
    /// Layers are merged from lowest priority to highest
    pub fn composite(&mut self) {
        // Clear the composited buffer
        for row in &mut self.composited {
            for cell in row.iter_mut() {
                *cell = Cell::empty();
            }
        }

        // Merge layers from lowest to highest priority
        // BTreeMap iterates in key order (ascending priority)
        for layer in self.layers.values() {
            for (r, row) in layer.cells.iter().enumerate() {
                if r >= self.rows {
                    break;
                }
                for (c, cell) in row.iter().enumerate() {
                    if c >= self.cols {
                        break;
                    }
                    if let Some(cell) = cell {
                        self.composited[r][c] = cell.clone();
                    }
                }
            }
        }

        // Mark all layers as clean
        for layer in self.layers.values_mut() {
            layer.mark_clean();
        }
        self.needs_composite = false;
    }

    /// Get the composited buffer
    /// Automatically composites if any layer is dirty
    pub fn get_composited(&mut self) -> &Vec<Vec<Cell>> {
        if self.needs_composite || self.layers.values().any(|l| l.is_dirty()) {
            self.composite();
        }
        &self.composited
    }

    /// Render the composited output to the terminal
    pub fn render_to_terminal<T: crate::term::TerminalBackend>(
        &mut self,
        term: &mut T,
        needs_clear: bool,
    ) -> Result<(), String> {
        use crossterm::queue;
        use crossterm::style::{ResetColor, SetBackgroundColor, SetForegroundColor};

        // Composite if needed
        if self.needs_composite || self.layers.values().any(|l| l.is_dirty()) {
            self.composite();
        }

        // Hide cursor during rendering
        term.hide_cursor()?;

        // Clear if needed
        if needs_clear {
            term.clear_screen()?;
        }

        // Track current colors to minimize escape sequences
        let mut current_fg: Option<Color> = None;
        let mut current_bg: Option<Color> = None;

        // Render each row
        for (row_idx, row) in self.composited.iter().enumerate() {
            term.move_cursor(row_idx as u16, 0)?;

            let mut output = Vec::with_capacity(self.cols * 4); // Estimate for UTF-8

            for cell in row {
                // Check if we need to change colors
                if cell.fg != current_fg || cell.bg != current_bg {
                    // Flush current output before color change
                    if !output.is_empty() {
                        term.write(&output)?;
                        output.clear();
                    }

                    // Build color commands
                    let mut color_buf = Vec::new();

                    // Reset and apply new colors
                    queue!(color_buf, ResetColor)
                        .map_err(|e| format!("Failed to reset colors: {e}"))?;

                    if let Some(fg) = cell.fg {
                        queue!(color_buf, SetForegroundColor(fg.to_crossterm()))
                            .map_err(|e| format!("Failed to set fg: {e}"))?;
                    }
                    if let Some(bg) = cell.bg {
                        queue!(color_buf, SetBackgroundColor(bg.to_crossterm()))
                            .map_err(|e| format!("Failed to set bg: {e}"))?;
                    }

                    term.write(&color_buf)?;
                    current_fg = cell.fg;
                    current_bg = cell.bg;
                }

                // Add cell content to output buffer
                output.extend_from_slice(&cell.content);
            }

            // Flush remaining output for this row
            if !output.is_empty() {
                term.write(&output)?;
            }

            // Clear to end of line
            term.clear_to_end_of_line()?;
        }

        // Reset colors at end
        let mut reset_buf = Vec::new();
        queue!(reset_buf, ResetColor).map_err(|e| format!("Failed to reset colors: {e}"))?;
        term.write(&reset_buf)?;

        Ok(())
    }

    /// Get the number of rows
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Get the number of columns
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Check if any layer needs recompositing
    pub fn needs_recomposite(&self) -> bool {
        self.needs_composite || self.layers.values().any(|l| l.is_dirty())
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
