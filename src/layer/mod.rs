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

use crate::character::Character;
use crate::color::Color;
use crate::screen_buffer::{DoubleBuffer, FrameStats};
use std::collections::BTreeMap;

/// A rectangular region
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
}

impl Rect {
    /// Create a new rectangle
    pub fn new(start_row: usize, start_col: usize, end_row: usize, end_col: usize) -> Self {
        Self {
            start_row,
            start_col,
            end_row,
            end_col,
        }
    }
    /// Check if the rect contains a point
    pub fn contains_point(&self, row: usize, col: usize) -> bool {
        row >= self.start_row && row <= self.end_row && col >= self.start_col && col <= self.end_col
    }

    /// Check if two rects intersect
    pub fn intersects(&self, other: &Rect) -> bool {
        self.start_row <= other.end_row
            && self.end_row >= other.start_row
            && self.start_col <= other.end_col
            && self.end_col >= other.start_col
    }

    /// Check if two rects are adjacent (touching)
    pub fn is_adjacent(&self, other: &Rect) -> bool {
        // Expand self by 1 in all directions and check for intersection
        let expanded = Rect {
            start_row: self.start_row.saturating_sub(1),
            start_col: self.start_col.saturating_sub(1),
            end_row: self.end_row.saturating_add(1),
            end_col: self.end_col.saturating_add(1),
        };
        expanded.intersects(other)
    }

    /// Union of two rects (bounding box)
    pub fn union(&self, other: &Rect) -> Rect {
        Rect {
            start_row: self.start_row.min(other.start_row),
            start_col: self.start_col.min(other.start_col),
            end_row: self.end_row.max(other.end_row),
            end_col: self.end_col.max(other.end_col),
        }
    }
}

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
    /// Notifications (highest priority)
    pub const NOTIFICATION: LayerPriority = LayerPriority(60);
}

/// A cell in the terminal buffer
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The character to display
    pub content: Character,
    /// Foreground color (None = default)
    pub fg: Option<Color>,
    /// Background color (None = default)
    pub bg: Option<Color>,
}

impl Cell {
    /// Create a new cell with the given character
    pub fn new(ch: Character) -> Self {
        Self {
            content: ch,
            fg: None,
            bg: None,
        }
    }

    /// Convert cell content to char (best effort)
    pub fn to_char(&self) -> char {
        self.content.to_char_lossy()
    }

    /// Create a new cell with UTF-8 content
    /// Note: This expects a single char bytes, or will map to Character
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if let Ok(s) = std::str::from_utf8(bytes) {
            if let Some(c) = s.chars().next() {
                return Self::new(Character::from(c));
            }
        }
        if !bytes.is_empty() {
            Self::new(Character::Byte(bytes[0]))
        } else {
            Self::empty()
        }
    }

    /// Create a new cell from a char
    pub fn from_char(ch: char) -> Self {
        Self::new(Character::from(ch))
    }

    /// Create an empty (space) cell
    pub fn empty() -> Self {
        Self::from_char(' ')
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
    /// Flat grid of cells - index = row * cols + col
    /// None = transparent, Some = content
    cells: Vec<Option<Cell>>,
    /// List of dirty rectangles that need compositing
    dirty_rects: Vec<Rect>,
    /// Number of rows in the layer
    rows: usize,
    /// Number of columns in the layer
    cols: usize,
}

impl Layer {
    /// Maximum number of dirty rects before collapsing to full layer
    const MAX_DIRTY_RECTS: usize = 10;

    /// Create a new layer with the given dimensions
    pub fn new(priority: LayerPriority, rows: usize, cols: usize) -> Self {
        let cells = vec![None; rows * cols];
        Self {
            priority,
            cells,
            // Initial state is fully dirty
            dirty_rects: vec![Rect::new(
                0,
                0,
                rows.saturating_sub(1),
                cols.saturating_sub(1),
            )],
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
        !self.dirty_rects.is_empty()
    }

    /// Get the list of dirty rectangles
    pub fn get_dirty_rects(&self) -> &[Rect] {
        &self.dirty_rects
    }

    /// Add a dirty rectangle to the tracking list
    pub fn add_dirty_rect(&mut self, rect: Rect) {
        let mut merged = false;
        for existing in &mut self.dirty_rects {
            if existing.intersects(&rect) || existing.is_adjacent(&rect) {
                *existing = existing.union(&rect);
                merged = true;
                break;
            }
        }

        if !merged {
            self.dirty_rects.push(rect);
        }

        // Compact list if too large
        while self.dirty_rects.len() > Self::MAX_DIRTY_RECTS {
            let mut best_pair = (0, 1);
            let mut min_growth = usize::MAX;

            for i in 0..self.dirty_rects.len() {
                for j in (i + 1)..self.dirty_rects.len() {
                    let r1 = self.dirty_rects[i];
                    let r2 = self.dirty_rects[j];
                    let union = r1.union(&r2);

                    let area_r1 = (r1.end_row - r1.start_row + 1) * (r1.end_col - r1.start_col + 1);
                    let area_r2 = (r2.end_row - r2.start_row + 1) * (r2.end_col - r2.start_col + 1);
                    let area_union = (union.end_row - union.start_row + 1)
                        * (union.end_col - union.start_col + 1);

                    let growth = area_union.saturating_sub(area_r1 + area_r2);
                    if growth < min_growth {
                        min_growth = growth;
                        best_pair = (i, j);
                    }
                }
            }

            // Merge best pair
            let (i, j) = best_pair;
            let r2 = self.dirty_rects.remove(j); // Remove higher index first
            let r1 = self.dirty_rects.remove(i);
            self.dirty_rects.push(r1.union(&r2));
        }
    }

    /// Mark the layer as clean (after compositing)
    pub fn mark_clean(&mut self) {
        self.dirty_rects.clear();
    }

    /// Mark the entire layer as dirty without changing content
    pub fn mark_fully_dirty(&mut self) {
        self.dirty_rects.clear();
        self.dirty_rects.push(Rect::new(
            0,
            0,
            self.rows.saturating_sub(1),
            self.cols.saturating_sub(1),
        ));
    }

    /// Clear all cells in the layer (make transparent)
    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = None;
        }
        self.dirty_rects.clear();
        self.dirty_rects.push(Rect::new(
            0,
            0,
            self.rows.saturating_sub(1),
            self.cols.saturating_sub(1),
        ));
    }

    /// Helper to get index from row/col
    #[inline]
    fn idx(&self, row: usize, col: usize) -> usize {
        row * self.cols + col
    }

    /// Set a cell at the given position
    /// Returns false if position is out of bounds
    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) -> bool {
        if row < self.rows && col < self.cols {
            let idx = self.idx(row, col);
            // Optimization: only mark dirty if cell actually changed
            let changed = match &self.cells[idx] {
                Some(current) => current != &cell,
                None => true,
            };

            if changed {
                self.cells[idx] = Some(cell);
                self.add_dirty_rect(Rect::new(row, col, row, col));
            }
            true
        } else {
            false
        }
    }

    /// Set a cell to transparent at the given position
    pub fn clear_cell(&mut self, row: usize, col: usize) -> bool {
        if row < self.rows && col < self.cols {
            let idx = self.idx(row, col);
            let changed = self.cells[idx].is_some();
            if changed {
                self.cells[idx] = None;
                self.add_dirty_rect(Rect::new(row, col, row, col));
            }
            true
        } else {
            false
        }
    }

    /// Get a cell at the given position
    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        if row < self.rows && col < self.cols {
            let idx = self.idx(row, col);
            self.cells[idx].as_ref()
        } else {
            None
        }
    }

    /// Write a string of text at the given position from a string slice
    pub fn write_str(&mut self, row: usize, start_col: usize, text: &str) {
        for (i, ch) in text.chars().enumerate() {
            let col = start_col + i;
            if col < self.cols {
                self.set_cell(row, col, Cell::from_char(ch));
            }
        }
    }

    /// Write a string of text with colors at the given position
    pub fn write_str_colored(
        &mut self,
        row: usize,
        start_col: usize,
        text: &str,
        fg: Option<Color>,
        bg: Option<Color>,
    ) {
        for (i, ch) in text.chars().enumerate() {
            let col = start_col + i;
            if col < self.cols {
                self.set_cell(row, col, Cell::from_char(ch).with_colors(fg, bg));
            }
        }
    }

    /// Write UTF-8 content at the given position
    /// Handles multi-byte characters by putting them in a single cell
    pub fn write_utf8(&mut self, row: usize, col: usize, content: &[u8]) -> bool {
        // Parse content to string then Character
        let s = String::from_utf8_lossy(content);
        if let Some(c) = s.chars().next() {
            self.set_cell(row, col, Cell::from_char(c))
        } else {
            false
        }
    }

    /// Write a character at the given position
    pub fn write_char(&mut self, row: usize, col: usize, ch: char) -> bool {
        self.set_cell(row, col, Cell::from_char(ch))
    }

    /// Write a Character at the given position
    pub fn write_character(&mut self, row: usize, col: usize, ch: Character) -> bool {
        self.set_cell(row, col, Cell::new(ch))
    }

    /// Fill a row with a character
    pub fn fill_row(&mut self, row: usize, ch: char, fg: Option<Color>, bg: Option<Color>) {
        if row < self.rows {
            for col in 0..self.cols {
                self.set_cell(row, col, Cell::from_char(ch).with_colors(fg, bg));
            }
        }
    }

    /// Fill a rectangular region with a cell
    pub fn fill_rect(&mut self, rect: Rect, cell: Cell) {
        for row in rect.start_row..=rect.end_row.min(self.rows.saturating_sub(1)) {
            for col in rect.start_col..=rect.end_col.min(self.cols.saturating_sub(1)) {
                self.set_cell(row, col, cell.clone());
            }
        }
    }

    /// Resize the layer to new dimensions
    /// Content is preserved where possible
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        let mut new_cells = vec![None; new_rows * new_cols];

        // Copy existing content
        for r in 0..self.rows.min(new_rows) {
            for c in 0..self.cols.min(new_cols) {
                let old_idx = self.idx(r, c);
                let new_idx = r * new_cols + c;
                new_cells[new_idx] = self.cells[old_idx].clone();
            }
        }

        self.cells = new_cells;
        self.rows = new_rows;
        self.cols = new_cols;
        self.dirty_rects.clear();
        self.dirty_rects.push(Rect::new(
            0,
            0,
            new_rows.saturating_sub(1),
            new_cols.saturating_sub(1),
        ));
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
    /// Double buffer for rendering
    buffer: DoubleBuffer,
}

impl LayerCompositor {
    /// Create a new compositor with the given terminal dimensions
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            layers: BTreeMap::new(),
            rows,
            cols,
            buffer: DoubleBuffer::new(rows, cols),
        }
    }

    /// Get or create a layer with the given priority
    pub fn get_layer_mut(&mut self, priority: LayerPriority) -> &mut Layer {
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
        // Removing a layer may reveal content underneath.
        // For now, mark all other layers as dirty to ensure a full re-composite.

        let removed = self.layers.remove(&priority);
        if removed.is_some() {
            // Mark all remaining layers as fully dirty to ensure correct composition.
            for layer in self.layers.values_mut() {
                layer.mark_fully_dirty();
            }
        }
        removed
    }

    /// Explicitly mark a layer as dirty (clears it for repopulation)
    pub fn mark_dirty(&mut self, priority: LayerPriority) {
        self.clear_layer(priority);
    }

    /// Clear all layers
    pub fn clear_all(&mut self) {
        for layer in self.layers.values_mut() {
            layer.clear();
        }
    }

    /// Clear a specific layer
    pub fn clear_layer(&mut self, priority: LayerPriority) {
        if let Some(layer) = self.layers.get_mut(&priority) {
            layer.clear();
        }
    }

    /// Resize all layers to new dimensions
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        self.rows = new_rows;
        self.cols = new_cols;
        self.buffer.resize(new_rows, new_cols);

        for layer in self.layers.values_mut() {
            layer.resize(new_rows, new_cols);
        }
    }

    /// Check if any layer has dirty rects
    pub fn has_dirty(&self) -> bool {
        self.layers.values().any(|l| l.is_dirty())
    }

    /// Composite all layers into the output buffer
    /// Layers are merged from lowest priority to highest
    pub fn composite(&mut self) {
        // Collect all dirty rects from all layers
        // We do this to know WHICH pixels need updating.
        let mut dirty_rects = Vec::new();
        for layer in self.layers.values() {
            dirty_rects.extend_from_slice(&layer.dirty_rects);
        }

        if dirty_rects.is_empty() {
            return;
        }

        // Process each dirty rect
        for rect in dirty_rects {
            // Iterate over every pixel in the dirty rect
            for r in rect.start_row..=rect.end_row {
                if r >= self.rows {
                    continue;
                }
                for c in rect.start_col..=rect.end_col {
                    if c >= self.cols {
                        continue;
                    }

                    // Re-evaluate this pixel's final value by iterating Top-Down (highest priority first)
                    // This allows early exit (occlusion culling)
                    let mut final_cell: Option<Cell> = None;

                    for layer in self.layers.values().rev() {
                        if let Some(cell) = layer.get_cell(r, c) {
                            final_cell = Some(cell.clone());
                            break;
                        }
                    }

                    if let Some(cell) = final_cell {
                        self.buffer.set_cell(r, c, cell);
                    } else {
                        self.buffer.set_cell(r, c, Cell::empty());
                    }
                }
            }
        }

        // Mark all layers as clean
        for layer in self.layers.values_mut() {
            layer.mark_clean();
        }
    }

    /// Get the composited buffer (read-only)
    /// Automatically composites if any layer is dirty
    pub fn get_composited_slice(&mut self) -> &[Cell] {
        if self.has_dirty() {
            self.composite();
        }
        self.buffer.current_slice()
    }

    /// Render the composited output to the terminal using double buffering
    /// Only cells that changed since the last frame are rendered
    pub fn render_to_terminal<T: crate::term::TerminalBackend>(
        &mut self,
        term: &mut T,
        needs_clear: bool,
    ) -> Result<FrameStats, String> {
        // Composite if needed
        if self.has_dirty() {
            self.composite();
        }

        // Force full redraw if requested
        if needs_clear {
            self.buffer.invalidate();
        }

        // Delegate to double buffer
        self.buffer.render_to_terminal(term)
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
        self.has_dirty()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
