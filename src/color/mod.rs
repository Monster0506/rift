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
            "reset" => Self::Reset,
            hex if hex.starts_with('#') && hex.len() == 7 => {
                let r = u8::from_str_radix(hex.get(1..3)?, 16).ok()?;
                let g = u8::from_str_radix(hex.get(3..5)?, 16).ok()?;
                let b = u8::from_str_radix(hex.get(5..7)?, 16).ok()?;
                Self::Rgb { r, g, b }
            }
            ansi if ansi.starts_with("ansi") => Self::Ansi256(ansi.get(4..)?.parse().ok()?),
            _ => return None,
        })
    }

    #[must_use]
    pub(crate) fn name(self) -> String {
        match self {
            Self::Reset => "reset".into(),
            Self::Black => "black".into(),
            Self::DarkGrey => "darkgrey".into(),
            Self::Red => "red".into(),
            Self::DarkRed => "darkred".into(),
            Self::Green => "green".into(),
            Self::DarkGreen => "darkgreen".into(),
            Self::Yellow => "yellow".into(),
            Self::DarkYellow => "darkyellow".into(),
            Self::Blue => "blue".into(),
            Self::DarkBlue => "darkblue".into(),
            Self::Magenta => "magenta".into(),
            Self::DarkMagenta => "darkmagenta".into(),
            Self::Cyan => "cyan".into(),
            Self::DarkCyan => "darkcyan".into(),
            Self::White => "white".into(),
            Self::Grey => "grey".into(),
            Self::Ansi256(n) => format!("ansi{n}"),
            Self::Rgb { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
        }
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
