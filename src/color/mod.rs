#[cfg(test)]
mod buffer;
#[cfg(test)]
mod styled;
pub(crate) mod theme;

type ColorPair = (Option<Color>, Option<Color>);

pub(crate) type CellColorSpan = (std::ops::Range<usize>, ColorPair);

pub(crate) type CellColorSpans = Vec<CellColorSpan>;

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
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s.to_lowercase().as_str() {
            "red" => Self::Red,
            "darkred" => Self::DarkRed,
            "green" => Self::Green,
            "darkgreen" => Self::DarkGreen,
            "blue" => Self::Blue,
            "darkblue" => Self::DarkBlue,
            "yellow" => Self::Yellow,
            "darkyellow" => Self::DarkYellow,
            "cyan" => Self::Cyan,
            "darkcyan" => Self::DarkCyan,
            "magenta" => Self::Magenta,
            "darkmagenta" => Self::DarkMagenta,
            "white" => Self::White,
            "black" => Self::Black,
            "grey" | "gray" => Self::Grey,
            "darkgrey" | "darkgray" => Self::DarkGrey,
            hex if hex.starts_with('#') && hex.len() == 7 => {
                let r = u8::from_str_radix(hex.get(1..3)?, 16).ok()?;
                let g = u8::from_str_radix(hex.get(3..5)?, 16).ok()?;
                let b = u8::from_str_radix(hex.get(5..7)?, 16).ok()?;
                Self::Rgb { r, g, b }
            }
            _ => return None,
        })
    }
}

#[must_use]
pub(crate) fn contrasting_color(bg: Color) -> Color {
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
            let lum = 0.114f32.mul_add(
                f32::from(b),
                0.299f32.mul_add(f32::from(r), 0.587 * f32::from(g)),
            );
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
            let lum = 0.114f32.mul_add(
                f32::from(b),
                0.299f32.mul_add(f32::from(r), 0.587 * f32::from(g)),
            );
            if lum > 128.0 {
                Color::Black
            } else {
                Color::White
            }
        }
        Color::Reset => Color::Reset,
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColorStyle {
    fg: Option<Color>,
    bg: Option<Color>,
}

#[cfg(test)]
impl ColorStyle {
    #[must_use]
    const fn new() -> Self {
        Self { fg: None, bg: None }
    }

    #[must_use]
    const fn fg(fg: Color) -> Self {
        Self {
            fg: Some(fg),
            bg: None,
        }
    }

    #[must_use]
    const fn bg(bg: Color) -> Self {
        Self {
            fg: None,
            bg: Some(bg),
        }
    }

    #[must_use]
    const fn new_colors(fg: Color, bg: Color) -> Self {
        Self {
            fg: Some(fg),
            bg: Some(bg),
        }
    }

    #[must_use]
    const fn is_empty(self) -> bool {
        self.fg.is_none() && self.bg.is_none()
    }
}

#[cfg(test)]
impl Default for ColorStyle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
