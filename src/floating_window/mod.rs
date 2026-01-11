//! Floating window component
//! Reusable overlay window that renders to layers
//!
//! ## `floating_window`/ Invariants
//!
//! - Floating windows never mutate editor or buffer state.
//! - Floating windows are positioned relative to layer/terminal coordinates.
//! - Window content is provided externally and rendered as-is.
//! - Window rendering is layer-native - always renders to a Layer.
//! - Window dimensions are constrained to layer/terminal size.
//! - Window position is validated to ensure it fits within bounds.

use crate::color::Color;
use crate::constants;

use crate::character::Character;
use crate::layer::{Cell, Layer};

/// Internal layout context for rendering
struct RenderLayout {
    start_row: usize,
    start_col: usize,
    width: usize,
    height: usize,
}

/// Border characters for floating windows
pub use constants::ui::borders::BorderSet as BorderChars;

impl Default for BorderChars {
    /// Create default border characters (Unicode box drawing)
    fn default() -> Self {
        constants::ui::borders::UNICODE
    }
}

impl BorderChars {
    /// Create border characters from byte slices
    #[must_use]
    pub fn new(
        top_left: char,
        top_right: char,
        bottom_left: char,
        bottom_right: char,
        horizontal: char,
        vertical: char,
    ) -> Self {
        constants::ui::borders::BorderSet {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            horizontal,
            vertical,
        }
    }

    /// Create border characters from single-byte ASCII characters
    #[must_use]
    pub fn from_ascii(
        top_left: char,
        top_right: char,
        bottom_left: char,
        bottom_right: char,
        horizontal: char,
        vertical: char,
    ) -> Self {
        constants::ui::borders::BorderSet {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            horizontal,
            vertical,
        }
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

/// Window style configuration
#[derive(Debug, Clone)]
pub struct WindowStyle {
    /// Whether to draw a border around the window
    pub border: bool,
    /// Custom border characters (None uses defaults)
    pub border_chars: Option<BorderChars>,
    /// Foreground color for window content
    pub fg: Option<Color>,
    /// Background color for window content
    pub bg: Option<Color>,
    /// Whether to use reverse video (swap fg/bg to black/white)
    pub reverse_video: bool,
}

impl Default for WindowStyle {
    fn default() -> Self {
        Self {
            border: true,
            border_chars: None,
            fg: None,
            bg: None,
            reverse_video: true,
        }
    }
}

impl WindowStyle {
    /// Create a new window style with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether to draw a border
    #[must_use]
    pub fn with_border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }

    /// Set custom border characters
    #[must_use]
    pub fn with_border_chars(mut self, border_chars: BorderChars) -> Self {
        self.border_chars = Some(border_chars);
        self
    }

    /// Set foreground color
    #[must_use]
    pub fn with_fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }

    /// Set background color
    #[must_use]
    pub fn with_bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }

    /// Set reverse video mode
    #[must_use]
    pub fn with_reverse_video(mut self, reverse: bool) -> Self {
        self.reverse_video = reverse;
        self
    }

    /// Get the effective colors (applying reverse video if set)
    fn effective_colors(&self) -> (Option<Color>, Option<Color>) {
        if self.reverse_video {
            (
                Some(self.bg.unwrap_or(Color::Black)),
                Some(self.fg.unwrap_or(Color::White)),
            )
        } else {
            (self.fg, self.bg)
        }
    }

    /// Get border chars (use custom or default)
    fn border_chars(&self) -> BorderChars {
        self.border_chars.clone().unwrap_or_default()
    }
}

/// Floating window configuration
///
/// A floating window is a rectangular overlay that can be rendered on a layer.
/// It supports optional borders, custom colors, and various positioning options.
#[derive(Debug, Clone)]
pub struct FloatingWindow {
    /// Window position
    position: WindowPosition,
    /// Window width in columns (includes border if enabled)
    width: usize,
    /// Window height in rows (includes border if enabled)
    height: usize,
    /// Window styling
    style: WindowStyle,
}

impl FloatingWindow {
    /// Create a new floating window with default style
    #[must_use]
    pub fn new(position: WindowPosition, width: usize, height: usize) -> Self {
        FloatingWindow {
            position,
            width,
            height,
            style: WindowStyle::default(),
        }
    }

    /// Create a new floating window with custom style
    #[must_use]
    pub fn with_style(
        position: WindowPosition,
        width: usize,
        height: usize,
        style: WindowStyle,
    ) -> Self {
        FloatingWindow {
            position,
            width,
            height,
            style,
        }
    }

    /// Set whether to draw a border (builder pattern for compatibility)
    #[must_use]
    pub fn with_border(mut self, border: bool) -> Self {
        self.style.border = border;
        self
    }

    /// Set whether to use reverse video (builder pattern for compatibility)
    #[must_use]
    pub fn with_reverse_video(mut self, reverse: bool) -> Self {
        self.style.reverse_video = reverse;
        self
    }

    /// Set custom border characters (builder pattern for compatibility)
    #[must_use]
    pub fn with_border_chars(mut self, border_chars: BorderChars) -> Self {
        self.style.border_chars = Some(border_chars);
        self
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

    /// Get the content width (width minus border if enabled)
    #[must_use]
    pub fn content_width(&self) -> usize {
        if self.style.border {
            self.width.saturating_sub(2)
        } else {
            self.width
        }
    }

    /// Get the content height (height minus border if enabled)
    #[must_use]
    pub fn content_height(&self) -> usize {
        if self.style.border {
            self.height.saturating_sub(2)
        } else {
            self.height
        }
    }

    /// Calculate the actual position of the window given layer/terminal dimensions
    /// Returns (row, col) where the window should be positioned (0-indexed)
    #[must_use]
    pub fn calculate_position(&self, rows: u16, cols: u16) -> (u16, u16) {
        let width = self.width.min(cols as usize) as u16;
        let height = self.height.min(rows as usize) as u16;

        match self.position {
            WindowPosition::Center => {
                let row = (rows.saturating_sub(height)) / 2;
                let col = (cols.saturating_sub(width)) / 2;
                (row, col)
            }
            WindowPosition::Absolute { row, col } => {
                // Clamp to bounds
                let row = row.min(rows.saturating_sub(height));
                let col = col.min(cols.saturating_sub(width));
                (row, col)
            }
            WindowPosition::Bottom => {
                let row = rows.saturating_sub(height);
                let col = (cols.saturating_sub(width)) / 2;
                (row, col)
            }
            WindowPosition::Top => {
                let row = 0;
                let col = (cols.saturating_sub(width)) / 2;
                (row, col)
            }
        }
    }

    /// Render the floating window to a layer
    ///
    /// This is the primary rendering method. The window is rendered to the provided
    /// layer at the calculated position based on the layer dimensions.
    ///
    /// # Arguments
    /// * `layer` - The layer to render to
    /// * `content` - Content lines (each line is a byte vector)
    pub fn render(&self, layer: &mut Layer, content: &[Vec<char>]) {
        self.render_with_border_chars(layer, content, None)
    }

    /// Render the floating window with Cells (colored content)
    pub fn render_cells(&self, layer: &mut Layer, content: &[Vec<Cell>]) {
        self.render_cells_with_border_chars(layer, content, None)
    }

    /// Render the floating window to a layer with optional border character override
    ///
    /// # Arguments
    /// * `layer` - The layer to render to
    /// * `content` - Content lines (each line is a byte vector)
    /// * `border_chars_override` - Optional override for border characters
    pub fn render_with_border_chars(
        &self,
        layer: &mut Layer,
        content: &[Vec<char>],
        border_chars_override: Option<BorderChars>,
    ) {
        let rows = layer.rows() as u16;
        let cols = layer.cols() as u16;

        // Calculate position
        let (start_row, start_col) = self.calculate_position(rows, cols);
        let start_row = start_row as usize;
        let start_col = start_col as usize;

        // Clamp dimensions to layer size
        let width = self.width.min(cols as usize);
        let height = self.height.min(rows as usize);

        // Get colors
        let (fg, bg) = self.style.effective_colors();

        // Get border chars
        let border_chars = border_chars_override.unwrap_or_else(|| self.style.border_chars());

        if self.style.border {
            self.render_with_border(
                layer,
                content,
                RenderLayout {
                    start_row,
                    start_col,
                    width,
                    height,
                },
                fg,
                bg,
                &border_chars,
            );
        } else {
            self.render_without_border(
                layer,
                content,
                RenderLayout {
                    start_row,
                    start_col,
                    width,
                    height,
                },
                fg,
                bg,
            );
        }
    }

    /// Render with custom border chars and Cells
    pub fn render_cells_with_border_chars(
        &self,
        layer: &mut Layer,
        content: &[Vec<Cell>],
        border_chars_override: Option<BorderChars>,
    ) {
        let rows = layer.rows() as u16;
        let cols = layer.cols() as u16;

        let (start_row, start_col) = self.calculate_position(rows, cols);
        let start_row = start_row as usize;
        let start_col = start_col as usize;

        let width = self.width.min(cols as usize);
        let height = self.height.min(rows as usize);

        let (fg, bg) = self.style.effective_colors();
        let border_chars = border_chars_override.unwrap_or_else(|| self.style.border_chars());

        if self.style.border {
            self.render_cells_with_border(
                layer,
                content,
                RenderLayout {
                    start_row,
                    start_col,
                    width,
                    height,
                },
                fg,
                bg,
                &border_chars,
            );
        } else {
            self.render_cells_without_border(
                layer,
                content,
                RenderLayout {
                    start_row,
                    start_col,
                    width,
                    height,
                },
                fg,
                bg,
            );
        }
    }

    /// Render window content with border (Cells version)
    fn render_cells_with_border(
        &self,
        layer: &mut Layer,
        content: &[Vec<Cell>],
        layout: RenderLayout,
        fg: Option<Color>,
        bg: Option<Color>,
        border_chars: &BorderChars,
    ) {
        let content_height = layout.height.saturating_sub(2);
        let content_width = layout.width.saturating_sub(2);
        let start_row = layout.start_row;
        let start_col = layout.start_col;
        let width = layout.width;
        let height = layout.height;

        // Top border
        layer.set_cell(
            start_row,
            start_col,
            Cell::from_char(border_chars.top_left).with_colors(fg, bg),
        );
        for i in 0..content_width {
            let col = start_col + 1 + i;
            layer.set_cell(
                start_row,
                col,
                Cell::from_char(border_chars.horizontal).with_colors(fg, bg),
            );
        }
        if width > 1 {
            let col = start_col + width - 1;
            layer.set_cell(
                start_row,
                col,
                Cell::from_char(border_chars.top_right).with_colors(fg, bg),
            );
        }

        // Content rows with side borders
        for content_row in 0..content_height {
            let row = start_row + 1 + content_row;

            // Left border
            layer.set_cell(
                row,
                start_col,
                Cell::from_char(border_chars.vertical).with_colors(fg, bg),
            );

            // Content
            if let Some(line) = content.get(content_row) {
                for (i, cell) in line.iter().take(content_width).enumerate() {
                    let col = start_col + 1 + i;
                    let mut final_cell = cell.clone();
                    if final_cell.fg.is_none() {
                        final_cell.fg = fg;
                    }
                    if final_cell.bg.is_none() {
                        final_cell.bg = bg;
                    }
                    layer.set_cell(row, col, final_cell);
                }
                // Pad with spaces
                for i in line.len().min(content_width)..content_width {
                    let col = start_col + 1 + i;
                    layer.set_cell(
                        row,
                        col,
                        Cell::new(Character::from(' ')).with_colors(fg, bg),
                    );
                }
            } else {
                // Empty line - fill with spaces
                for i in 0..content_width {
                    let col = start_col + 1 + i;
                    layer.set_cell(
                        row,
                        col,
                        Cell::new(Character::from(' ')).with_colors(fg, bg),
                    );
                }
            }

            // Right border
            if width > 1 {
                let col = start_col + width - 1;
                layer.set_cell(
                    row,
                    col,
                    Cell::from_char(border_chars.vertical).with_colors(fg, bg),
                );
            }
        }

        // Bottom border
        if height > 1 {
            let row = start_row + height - 1;
            layer.set_cell(
                row,
                start_col,
                Cell::from_char(border_chars.bottom_left).with_colors(fg, bg),
            );
            for i in 0..content_width {
                let col = start_col + 1 + i;
                layer.set_cell(
                    row,
                    col,
                    Cell::from_char(border_chars.horizontal).with_colors(fg, bg),
                );
            }
            if width > 1 {
                let col = start_col + width - 1;
                layer.set_cell(
                    row,
                    col,
                    Cell::from_char(border_chars.bottom_right).with_colors(fg, bg),
                );
            }
        }
    }

    /// Render window content without border (Cells version)
    fn render_cells_without_border(
        &self,
        layer: &mut Layer,
        content: &[Vec<Cell>],
        layout: RenderLayout,
        fg: Option<Color>,
        bg: Option<Color>,
    ) {
        let start_row = layout.start_row;
        let start_col = layout.start_col;
        let height = layout.height;
        let width = layout.width;

        for row_offset in 0..height {
            let row = start_row + row_offset;

            if let Some(line) = content.get(row_offset) {
                for (i, cell) in line.iter().take(width).enumerate() {
                    let col = start_col + i;
                    let mut final_cell = cell.clone();
                    if final_cell.fg.is_none() {
                        final_cell.fg = fg;
                    }
                    if final_cell.bg.is_none() {
                        final_cell.bg = bg;
                    }
                    layer.set_cell(row, col, final_cell);
                }
                // Pad with spaces
                for i in line.len().min(width)..width {
                    let col = start_col + i;
                    layer.set_cell(row, col, Cell::from_char(' ').with_colors(fg, bg));
                }
            } else {
                // Empty line - fill with spaces
                for i in 0..width {
                    let col = start_col + i;
                    layer.set_cell(
                        row,
                        col,
                        Cell::new(Character::from(' ')).with_colors(fg, bg),
                    );
                }
            }
        }
    }

    /// Render window content with border
    fn render_with_border(
        &self,
        layer: &mut Layer,
        content: &[Vec<char>],
        layout: RenderLayout,
        fg: Option<Color>,
        bg: Option<Color>,
        border_chars: &BorderChars,
    ) {
        let content_height = layout.height.saturating_sub(2);
        let content_width = layout.width.saturating_sub(2);
        let start_row = layout.start_row;
        let start_col = layout.start_col;
        let width = layout.width;
        let height = layout.height;

        // Top border
        layer.set_cell(
            start_row,
            start_col,
            Cell::from_char(border_chars.top_left).with_colors(fg, bg),
        );
        for i in 0..content_width {
            let col = start_col + 1 + i;
            layer.set_cell(
                start_row,
                col,
                Cell::from_char(border_chars.horizontal).with_colors(fg, bg),
            );
        }
        if width > 1 {
            let col = start_col + width - 1;
            layer.set_cell(
                start_row,
                col,
                Cell::from_char(border_chars.top_right).with_colors(fg, bg),
            );
        }

        // Content rows with side borders
        for content_row in 0..content_height {
            let row = start_row + 1 + content_row;

            // Left border
            layer.set_cell(
                row,
                start_col,
                Cell::from_char(border_chars.vertical).with_colors(fg, bg),
            );

            // Content
            if let Some(line) = content.get(content_row) {
                for (i, &byte) in line.iter().take(content_width).enumerate() {
                    let col = start_col + 1 + i;
                    layer.set_cell(row, col, Cell::from_char(byte).with_colors(fg, bg));
                }
                // Pad with spaces
                for i in line.len().min(content_width)..content_width {
                    let col = start_col + 1 + i;
                    layer.set_cell(row, col, Cell::from_bytes(b" ").with_colors(fg, bg));
                }
            } else {
                // Empty line - fill with spaces
                for i in 0..content_width {
                    let col = start_col + 1 + i;
                    layer.set_cell(
                        row,
                        col,
                        Cell::new(Character::from(' ')).with_colors(fg, bg),
                    );
                }
            }

            // Right border
            if width > 1 {
                let col = start_col + width - 1;
                layer.set_cell(
                    row,
                    col,
                    Cell::from_char(border_chars.vertical).with_colors(fg, bg),
                );
            }
        }

        // Bottom border
        if height > 1 {
            let row = start_row + height - 1;
            layer.set_cell(
                row,
                start_col,
                Cell::from_char(border_chars.bottom_left).with_colors(fg, bg),
            );
            for i in 0..content_width {
                let col = start_col + 1 + i;
                layer.set_cell(
                    row,
                    col,
                    Cell::from_char(border_chars.horizontal).with_colors(fg, bg),
                );
            }
            if width > 1 {
                let col = start_col + width - 1;
                layer.set_cell(
                    row,
                    col,
                    Cell::from_char(border_chars.bottom_right).with_colors(fg, bg),
                );
            }
        }
    }

    /// Render window content without border
    fn render_without_border(
        &self,
        layer: &mut Layer,
        content: &[Vec<char>],
        layout: RenderLayout,
        fg: Option<Color>,
        bg: Option<Color>,
    ) {
        let start_row = layout.start_row;
        let start_col = layout.start_col;
        let height = layout.height;
        let width = layout.width;

        for row_offset in 0..height {
            let row = start_row + row_offset;

            if let Some(line) = content.get(row_offset) {
                for (i, &byte) in line.iter().take(width).enumerate() {
                    let col = start_col + i;
                    layer.set_cell(row, col, Cell::from_char(byte).with_colors(fg, bg));
                }
                // Pad with spaces
                for i in line.len().min(width)..width {
                    let col = start_col + i;
                    layer.set_cell(row, col, Cell::from_char(' ').with_colors(fg, bg));
                }
            } else {
                // Empty line - fill with spaces
                for i in 0..width {
                    let col = start_col + i;
                    layer.set_cell(
                        row,
                        col,
                        Cell::new(Character::from(' ')).with_colors(fg, bg),
                    );
                }
            }
        }
    }

    /// Render a single-line content (convenience method)
    ///
    /// `prompt` is displayed at the start, followed by `content`
    pub fn render_single_line(&self, layer: &mut Layer, prompt: &str, content: &str) {
        let mut line: Vec<char> = Vec::new();
        line.extend(prompt.chars());
        line.extend(content.chars());
        self.render(layer, &[line]);
    }

    // ========================================================================
    // Legacy compatibility methods (for direct terminal rendering)
    // These methods are for backward compatibility during transition
    // ========================================================================

    /// Render the floating window to a layer (legacy compatibility method)
    ///
    /// This method signature matches the previous `render_to_layer` for compatibility.
    /// Prefer using `render()` for new code.
    #[deprecated(note = "Use render() instead - this is for backward compatibility")]
    pub fn render_to_layer(
        &self,
        layer: &mut Layer,
        content: &[Vec<char>],
        _term_rows: u16,
        _term_cols: u16,
        border_chars_override: Option<BorderChars>,
    ) {
        self.render_with_border_chars(layer, content, border_chars_override);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
