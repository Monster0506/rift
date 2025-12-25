//! Color system
//! Provides color types, styles, and extension points for syntax highlighting

use crossterm::style::Color as CrosstermColor;

pub mod styled;
pub mod buffer;
pub mod theme;

pub use theme::{Theme, ThemeVariant};

/// Color representation wrapping crossterm's Color enum
/// Supports 16 colors, 256 colors, and RGB colors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Reset to default color
    Reset,
    /// Standard 16 colors
    Black,
    DarkGrey,
    Red,
    DarkRed,
    Green,
    DarkGreen,
    Yellow,
    DarkYellow,
    Blue,
    DarkBlue,
    Magenta,
    DarkMagenta,
    Cyan,
    DarkCyan,
    White,
    Grey,
    /// 256-color palette (0-255)
    Ansi256(u8),
    /// RGB color (r, g, b) where each component is 0-255
    Rgb { r: u8, g: u8, b: u8 },
}

impl Color {
    /// Convert to crossterm Color
    #[must_use] 
    pub fn to_crossterm(self) -> CrosstermColor {
        match self {
            Color::Reset => CrosstermColor::Reset,
            Color::Black => CrosstermColor::Black,
            Color::DarkGrey => CrosstermColor::DarkGrey,
            Color::Red => CrosstermColor::Red,
            Color::DarkRed => CrosstermColor::DarkRed,
            Color::Green => CrosstermColor::Green,
            Color::DarkGreen => CrosstermColor::DarkGreen,
            Color::Yellow => CrosstermColor::Yellow,
            Color::DarkYellow => CrosstermColor::DarkYellow,
            Color::Blue => CrosstermColor::Blue,
            Color::DarkBlue => CrosstermColor::DarkBlue,
            Color::Magenta => CrosstermColor::Magenta,
            Color::DarkMagenta => CrosstermColor::DarkMagenta,
            Color::Cyan => CrosstermColor::Cyan,
            Color::DarkCyan => CrosstermColor::DarkCyan,
            Color::White => CrosstermColor::White,
            Color::Grey => CrosstermColor::Grey,
            Color::Ansi256(n) => CrosstermColor::AnsiValue(n),
            Color::Rgb { r, g, b } => CrosstermColor::Rgb { r, g, b },
        }
    }

    /// Create from crossterm Color
    #[must_use] 
    pub fn from_crossterm(color: CrosstermColor) -> Self {
        match color {
            CrosstermColor::Reset => Color::Reset,
            CrosstermColor::Black => Color::Black,
            CrosstermColor::DarkGrey => Color::DarkGrey,
            CrosstermColor::Red => Color::Red,
            CrosstermColor::DarkRed => Color::DarkRed,
            CrosstermColor::Green => Color::Green,
            CrosstermColor::DarkGreen => Color::DarkGreen,
            CrosstermColor::Yellow => Color::Yellow,
            CrosstermColor::DarkYellow => Color::DarkYellow,
            CrosstermColor::Blue => Color::Blue,
            CrosstermColor::DarkBlue => Color::DarkBlue,
            CrosstermColor::Magenta => Color::Magenta,
            CrosstermColor::DarkMagenta => Color::DarkMagenta,
            CrosstermColor::Cyan => Color::Cyan,
            CrosstermColor::DarkCyan => Color::DarkCyan,
            CrosstermColor::White => Color::White,
            CrosstermColor::Grey => Color::Grey,
            CrosstermColor::AnsiValue(n) => Color::Ansi256(n),
            CrosstermColor::Rgb { r, g, b } => Color::Rgb { r, g, b },
        }
    }
}

/// Color style combining foreground and background colors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorStyle {
    /// Foreground color (None means default/unchanged)
    pub fg: Option<Color>,
    /// Background color (None means default/unchanged)
    pub bg: Option<Color>,
}

impl ColorStyle {
    /// Create a new color style
    #[must_use] 
    pub fn new() -> Self {
        ColorStyle {
            fg: None,
            bg: None,
        }
    }

    /// Create with foreground color only
    #[must_use] 
    pub fn fg(fg: Color) -> Self {
        ColorStyle {
            fg: Some(fg),
            bg: None,
        }
    }

    /// Create with background color only
    #[must_use] 
    pub fn bg(bg: Color) -> Self {
        ColorStyle {
            fg: None,
            bg: Some(bg),
        }
    }

    /// Create with both foreground and background colors
    #[must_use] 
    pub fn new_colors(fg: Color, bg: Color) -> Self {
        ColorStyle {
            fg: Some(fg),
            bg: Some(bg),
        }
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

    /// Check if style has any colors set
    #[must_use] 
    pub fn is_empty(&self) -> bool {
        self.fg.is_none() && self.bg.is_none()
    }
}

impl Default for ColorStyle {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait for syntax highlighting
/// Future syntax highlighters can implement this trait
pub trait SyntaxHighlighter {
    /// Get the color style for a character at the given position
    /// 
    /// Returns None if no special styling should be applied
    fn get_style(&self, line: usize, column: usize) -> Option<ColorStyle>;

    /// Get color spans for an entire line
    /// 
    /// This is more efficient than calling `get_style` for each character
    /// Returns a vector of (`start_col`, `end_col`, style) tuples
    fn get_line_spans(&self, line: usize, line_length: usize) -> Vec<(usize, usize, ColorStyle)> {
        let mut spans = Vec::new();
        let mut current_start = 0;
        let mut current_style = None;

        for col in 0..line_length {
            let style = self.get_style(line, col);
            
            if style != current_style {
                // End current span if it exists
                if let Some(style) = current_style {
                    spans.push((current_start, col, style));
                }
                
                // Start new span
                current_start = col;
                current_style = style;
            }
        }

        // Add final span if exists
        if let Some(style) = current_style {
            spans.push((current_start, line_length, style));
        }

        spans
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;


