//! Layer compositing: UI components render independently onto z-ordered
//! layers (lowest first); transparent cells show through to the layer below.

use crate::character::Character;
use crate::color::Color;
use crate::error::RiftError;
use crate::screen_buffer::{DoubleBuffer, FrameStats};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
    #[must_use]
    pub const fn new(start_row: usize, start_col: usize, end_row: usize, end_col: usize) -> Self {
        Self {
            start_row,
            start_col,
            end_row,
            end_col,
        }
    }
    /// Check if two rects intersect
    #[must_use]
    pub const fn intersects(&self, other: &Self) -> bool {
        self.start_row <= other.end_row
            && self.end_row >= other.start_row
            && self.start_col <= other.end_col
            && self.end_col >= other.start_col
    }

    /// Check if two rects are adjacent (touching)
    #[must_use]
    pub const fn is_adjacent(&self, other: &Self) -> bool {
        // Expand self by 1 in all directions and check for intersection
        let expanded = Self {
            start_row: self.start_row.saturating_sub(1),
            start_col: self.start_col.saturating_sub(1),
            end_row: self.end_row.saturating_add(1),
            end_col: self.end_col.saturating_add(1),
        };
        expanded.intersects(other)
    }

    /// Union of two rects (bounding box)
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            start_row: self.start_row.min(other.start_row),
            start_col: self.start_col.min(other.start_col),
            end_row: self.end_row.max(other.end_row),
            end_col: self.end_col.max(other.end_col),
        }
    }
}

/// Repeatedly union overlapping/touching rects so a full invalidation from
/// several layers (e.g. identical full-screen rects) composites once.
fn merge_dirty_rects(mut rects: Vec<Rect>) -> Vec<Rect> {
    loop {
        let mut merged_any = false;
        'outer: for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                if rects[i].intersects(&rects[j]) || rects[i].is_adjacent(&rects[j]) {
                    rects[i] = rects[i].union(&rects[j]);
                    rects.remove(j);
                    merged_any = true;
                    break 'outer;
                }
            }
        }
        if !merged_any {
            break;
        }
    }
    rects
}

/// Layer priority - higher values render on top
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayerPriority(pub u8);

impl LayerPriority {
    /// Main text buffer content
    pub const CONTENT: Self = Self(0);
    /// Status line at the bottom of the screen
    pub const STATUS_BAR: Self = Self(10);
    /// Floating windows like command line, dialogs
    pub const FLOATING_WINDOW: Self = Self(20);
    /// Popup menus like autocomplete
    pub const POPUP: Self = Self(30);
    /// Hover information
    pub const HOVER: Self = Self(40);
    /// Tooltips and hints
    pub const TOOLTIP: Self = Self(50);
    /// Notifications
    pub const NOTIFICATION: Self = Self(60);
    /// Software cursor
    pub const CURSOR: Self = Self(100);
}

/// Text rendering attributes that compose on top of fg/bg color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
// One byte keeps `Cell` small and makes style equality a single integer compare.
pub struct CellAttrs(u8);

impl CellAttrs {
    pub const BOLD: Self = Self(0x01);
    pub const ITALIC: Self = Self(0x02);
    pub const UNDERLINE: Self = Self(0x04);
    pub const STRIKE: Self = Self(0x08);
    pub const REVERSE: Self = Self(0x10);

    /// Attribute names used by Lua style tables and serialized output, in bit order.
    pub(crate) const NAMED: [(&'static str, Self); 5] = [
        ("bold", Self::BOLD),
        ("italic", Self::ITALIC),
        ("underline", Self::UNDERLINE),
        ("strike", Self::STRIKE),
        ("reverse", Self::REVERSE),
    ];

    #[must_use]
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// True when every attribute in `other` is also set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Both sets of attributes (an underline composes over syntax bold/italic).
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn is_bold(self) -> bool {
        self.contains(Self::BOLD)
    }

    #[must_use]
    pub const fn is_italic(self) -> bool {
        self.contains(Self::ITALIC)
    }

    #[must_use]
    pub const fn is_underline(self) -> bool {
        self.contains(Self::UNDERLINE)
    }

    #[must_use]
    pub const fn is_strike(self) -> bool {
        self.contains(Self::STRIKE)
    }

    #[must_use]
    pub const fn is_reverse(self) -> bool {
        self.contains(Self::REVERSE)
    }
}

impl Serialize for CellAttrs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(
            Self::NAMED
                .iter()
                .filter(|(_, flag)| self.contains(*flag))
                .map(|(name, _)| *name),
        )
    }
}

impl<'de> Deserialize<'de> for CellAttrs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let names = Vec::<String>::deserialize(deserializer)?;
        let mut attrs = Self::default();
        for name in names {
            let Some(&(_, flag)) = Self::NAMED.iter().find(|(known, _)| *known == name) else {
                return Err(D::Error::custom(format!("unknown text attribute `{name}`")));
            };
            attrs = attrs.union(flag);
        }
        Ok(attrs)
    }
}

/// A fully-resolved cell style: optional fg/bg plus text attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CellStyle {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub attrs: CellAttrs,
}

/// A cell in the terminal buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The character to display
    pub content: Character,
    /// Foreground color (None = default)
    pub fg: Option<Color>,
    /// Background color (None = default)
    pub bg: Option<Color>,
    /// Text attributes (bold/italic/underline/strike/reverse).
    pub attrs: CellAttrs,
}

impl Cell {
    /// Create a new cell with the given character
    #[must_use]
    pub fn new(ch: Character) -> Self {
        Self {
            content: ch,
            fg: None,
            bg: None,
            attrs: CellAttrs::default(),
        }
    }

    /// Convert cell content to char (best effort)
    #[must_use]
    pub const fn to_char(&self) -> char {
        self.content.to_char_lossy()
    }

    /// Create a new cell with UTF-8 content
    /// Note: This expects a single char bytes, or will map to Character
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if let Ok(s) = std::str::from_utf8(bytes) {
            if let Some(c) = s.chars().next() {
                return Self::new(Character::from(c));
            }
        }
        if bytes.is_empty() {
            Self::empty()
        } else {
            Self::new(Character::Byte(bytes[0]))
        }
    }

    /// Create a new cell from a char
    #[must_use]
    pub fn from_char(ch: char) -> Self {
        Self::new(Character::from(ch))
    }

    /// Create an empty (space) cell
    #[must_use]
    pub fn empty() -> Self {
        Self::from_char(' ')
    }

    /// Set foreground color
    #[must_use]
    pub const fn with_fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }

    /// Set background color
    #[must_use]
    pub const fn with_bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }

    /// Set both foreground and background colors
    #[must_use]
    pub const fn with_colors(mut self, fg: Option<Color>, bg: Option<Color>) -> Self {
        self.fg = fg;
        self.bg = bg;
        self
    }

    /// Set text attributes.
    #[must_use]
    pub const fn with_attrs(mut self, attrs: CellAttrs) -> Self {
        self.attrs = attrs;
        self
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::empty()
    }
}

/// A single rendering layer: a grid of optional cells, `None` = transparent.
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
    #[must_use]
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
    #[must_use]
    pub const fn priority(&self) -> LayerPriority {
        self.priority
    }

    /// Check if the layer has been modified
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        !self.dirty_rects.is_empty()
    }

    /// Get the list of dirty rectangles
    #[must_use]
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
        for cell in &mut self.cells {
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
    const fn idx(&self, row: usize, col: usize) -> usize {
        row * self.cols + col
    }

    /// Set a cell at the given position
    /// Returns false if position is out of bounds
    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) -> bool {
        if row < self.rows && col < self.cols {
            let idx = self.idx(row, col);
            // Optimization: only mark dirty if cell actually changed
            let changed = self.cells[idx]
                .as_ref()
                .is_none_or(|current| current != &cell);

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
    #[must_use]
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
                self.set_cell(row, col, cell);
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
                new_cells[new_idx] = self.cells[old_idx];
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
    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    /// Get the number of columns
    #[must_use]
    pub const fn cols(&self) -> usize {
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
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            layers: BTreeMap::new(),
            rows,
            cols,
            buffer: DoubleBuffer::new(rows, cols),
        }
    }

    pub(crate) fn check_invariants(&self) -> Vec<String> {
        let mut out = self.buffer.check_invariants();
        if self.buffer.rows() != self.rows {
            out.push(format!(
                "compositor.rows={} != buffer.rows()={}",
                self.rows,
                self.buffer.rows()
            ));
        }
        if self.buffer.cols() != self.cols {
            out.push(format!(
                "compositor.cols={} != buffer.cols()={}",
                self.cols,
                self.buffer.cols()
            ));
        }
        out
    }

    /// Get or create a layer with the given priority
    pub fn get_layer_mut(&mut self, priority: LayerPriority) -> &mut Layer {
        self.layers
            .entry(priority)
            .or_insert_with(|| Layer::new(priority, self.rows, self.cols))
    }

    /// Get a layer by priority (read-only)
    #[must_use]
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
    #[must_use]
    pub fn has_dirty(&self) -> bool {
        self.layers.values().any(Layer::is_dirty)
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
        let dirty_rects = merge_dirty_rects(dirty_rects);

        // Process each dirty rect
        for rect in dirty_rects {
            let start_row = rect.start_row.saturating_sub(2);
            let start_col = rect.start_col.saturating_sub(2);
            let end_row = (rect.end_row + 2).min(self.rows.saturating_sub(1));
            let end_col = (rect.end_col + 2).min(self.cols.saturating_sub(1));
            // get_cell/set_cell below already no-op out of range.
            for r in start_row..=end_row {
                for c in start_col..=end_col {
                    // Re-evaluate this pixel's final value by iterating Top-Down (highest priority first)
                    // This allows early exit (occlusion culling)
                    let mut final_cell: Option<&Cell> = None;

                    for layer in self.layers.values().rev() {
                        if let Some(cell) = layer.get_cell(r, c) {
                            final_cell = Some(cell);
                            break;
                        }
                    }

                    // Skip the write (and its dirty-rect expansion) when
                    // recompositing this pixel didn't actually change it.
                    let unchanged = match (final_cell, self.buffer.get_cell(r, c)) {
                        (Some(new), Some(old)) => new == old,
                        (None, Some(old)) => *old == Cell::empty(),
                        _ => false,
                    };
                    if !unchanged {
                        let cell = final_cell.copied().unwrap_or_else(Cell::empty);
                        self.buffer.set_cell(r, c, cell);
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

    /// Get a single composited cell, compositing first if any layer is dirty.
    pub fn get_composited_cell(&mut self, row: usize, col: usize) -> Option<Cell> {
        if self.has_dirty() {
            self.composite();
        }
        if row < self.rows && col < self.cols {
            Some(self.buffer.current_slice()[row * self.cols + col])
        } else {
            None
        }
    }

    /// Render the composited output, diffing against the previous frame. A
    /// `(top, bottom, delta)` scroll hint rides the terminal's scroll region.
    ///
    /// # Errors
    /// Returns an error if the terminal backend fails to apply the scroll or write the frame.
    pub fn render_to_terminal<T: crate::term::TerminalBackend>(
        &mut self,
        term: &mut T,
        needs_clear: bool,
        scroll_hint: Option<(usize, usize, isize)>,
    ) -> Result<FrameStats, RiftError> {
        // Composite if needed
        if self.has_dirty() {
            self.composite();
        }

        // Force full redraw if requested
        if needs_clear {
            self.buffer.invalidate();
        } else if let Some((top, bottom, delta)) = scroll_hint {
            self.buffer.apply_scroll(term, top, bottom, delta)?;
        }

        // Delegate to double buffer
        self.buffer.render_to_terminal(term)
    }

    /// Get the number of rows
    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    /// Get the number of columns
    #[must_use]
    pub const fn cols(&self) -> usize {
        self.cols
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
