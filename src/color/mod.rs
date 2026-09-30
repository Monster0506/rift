pub mod buffer;
pub mod styled;
pub mod theme;

pub use theme::{Theme, ThemeVariant};

pub type ColorPair = (Option<Color>, Option<Color>);

pub type CellColorSpan = (std::ops::Range<usize>, ColorPair);

pub type CellColorSpans = Vec<CellColorSpan>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Color {
    Reset,
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
    Ansi256(u8),
    Rgb { r: u8, g: u8, b: u8 },
}

impl Color {
    #[must_use]
    pub fn parse(s: &str) -> Option<Color> {
        Some(match s.to_lowercase().as_str() {
            "red" => Color::Red,
            "darkred" => Color::DarkRed,
            "green" => Color::Green,
            "darkgreen" => Color::DarkGreen,
            "blue" => Color::Blue,
            "darkblue" => Color::DarkBlue,
            "yellow" => Color::Yellow,
            "darkyellow" => Color::DarkYellow,
            "cyan" => Color::Cyan,
            "darkcyan" => Color::DarkCyan,
            "magenta" => Color::Magenta,
            "darkmagenta" => Color::DarkMagenta,
            "white" => Color::White,
            "black" => Color::Black,
            "grey" | "gray" => Color::Grey,
            "darkgrey" | "darkgray" => Color::DarkGrey,
            hex if hex.starts_with('#') && hex.len() == 7 => {
                let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
                let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
                let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
                Color::Rgb { r, g, b }
            }
            _ => return None,
        })
    }
}

pub fn contrasting_color(bg: Color) -> Color {
    match bg {
        Color::Black
        | Color::DarkGrey
        | Color::Blue
        | Color::DarkBlue
        | Color::Red
        | Color::DarkRed
        | Color::Magenta
        | Color::DarkMagenta
        | Color::DarkGreen
        | Color::DarkCyan
        | Color::DarkYellow => Color::White,
        Color::White | Color::Grey | Color::Yellow | Color::Green | Color::Cyan => Color::Black,
        Color::Rgb { r, g, b } => {
            let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            if lum > 128.0 {
                Color::Black
            } else {
                Color::White
            }
        }
        Color::Ansi256(n) => {
            let (r, g, b) = if n >= 232 {
                let step = (n - 232) * 10 + 8;
                (step, step, step)
            } else {
                let i = n - 16;
                let ramp = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
                (ramp(i / 36), ramp((i / 6) % 6), ramp(i % 6))
            };
            let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            if lum > 128.0 {
                Color::Black
            } else {
                Color::White
            }
        }
        Color::Reset => Color::Reset,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorStyle {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

impl ColorStyle {
    #[must_use]
    pub fn new() -> Self {
        ColorStyle { fg: None, bg: None }
    }

    #[must_use]
    pub fn fg(fg: Color) -> Self {
        ColorStyle {
            fg: Some(fg),
            bg: None,
        }
    }

    #[must_use]
    pub fn bg(bg: Color) -> Self {
        ColorStyle {
            fg: None,
            bg: Some(bg),
        }
    }

    #[must_use]
    pub fn new_colors(fg: Color, bg: Color) -> Self {
        ColorStyle {
            fg: Some(fg),
            bg: Some(bg),
        }
    }

    #[must_use]
    pub fn with_fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }

    #[must_use]
    pub fn with_bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }

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

pub trait SyntaxHighlighter {
    fn get_style(&self, line: usize, column: usize) -> Option<ColorStyle>;

    fn get_line_spans(&self, line: usize, line_length: usize) -> Vec<(usize, usize, ColorStyle)> {
        let mut spans = Vec::new();
        let mut current_start = 0;
        let mut current_style = None;

        for col in 0..line_length {
            let style = self.get_style(line, col);

            if style != current_style {
                if let Some(style) = current_style {
                    spans.push((current_start, col, style));
                }

                current_start = col;
                current_style = style;
            }
        }

        if let Some(style) = current_style {
            spans.push((current_start, line_length, style));
        }

        spans
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
