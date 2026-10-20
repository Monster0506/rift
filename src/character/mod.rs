use std::fmt::{self, Display, Formatter};
use unicode_width::UnicodeWidthChar;

/// First C1 control code point; encodes as two UTF-8 bytes.
pub const C1_CONTROL_MIN: u8 = 0x80;

/// Bit toggled to map a control byte to its caret letter (0x01 -> 'A', 0x7F -> '?').
const CARET_FLIP: u8 = 0x40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Character {
    /// A Unicode scalar value
    Unicode(char),

    /// A raw byte that is not valid UTF-8
    Byte(u8),

    /// A horizontal tab
    Tab,

    /// A newline
    Newline,

    /// Control character (rendered visibly, e.g. ^C)
    Control(u8),
}

impl Character {
    /// Render the character to a formatter/output
    ///
    /// # Errors
    /// Returns an error if writing to `out` fails.
    pub fn render(&self, out: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::Unicode(c) => write!(out, "{c}"),
            Self::Byte(b) => write!(out, "\\x{b:02X}"),
            Self::Tab => write!(out, "\t"),
            Self::Newline => writeln!(out),
            Self::Control(b) if *b >= C1_CONTROL_MIN => write!(out, "<{b:02X}>"),
            Self::Control(b) => {
                if *b == b'\r' {
                    write!(out, "\\n")
                } else {
                    write!(out, "^{}", (b ^ CARET_FLIP) as char)
                }
            }
        }
    }

    /// Get the visual width of the character at a specific column
    #[must_use]
    pub fn render_width(&self, col: usize, tab_width: usize) -> usize {
        match self {
            Self::Unicode(c) => UnicodeWidthChar::width(*c).unwrap_or(0),
            Self::Byte(_) | Self::Control(C1_CONTROL_MIN..=u8::MAX) => 4, // \xNN or <NN>
            Self::Tab => tab_width - (col % tab_width),
            Self::Newline => 0,
            Self::Control(_) => 2, // ^C is 2 chars
        }
    }

    /// Get the logical width (unit of movement)
    #[must_use]
    pub const fn logical_width(&self) -> usize {
        1
    }

    /// Get the UTF-8 byte length for serialization
    #[must_use]
    pub const fn len_utf8(&self) -> usize {
        match self {
            Self::Unicode(c) => c.len_utf8(),
            Self::Control(b) if *b >= C1_CONTROL_MIN => 2,
            Self::Byte(_) | Self::Tab | Self::Newline | Self::Control(_) => 1,
        }
    }

    /// Encode the character as UTF-8 bytes into a buffer
    pub fn encode_utf8(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Unicode(c) => {
                let mut b = [0; 4];
                let s = c.encode_utf8(&mut b);
                buf.extend_from_slice(s.as_bytes());
            }
            Self::Control(b) if *b >= C1_CONTROL_MIN => {
                let mut utf8 = [0; 4];
                let s = char::from(*b).encode_utf8(&mut utf8);
                buf.extend_from_slice(s.as_bytes());
            }
            Self::Byte(b) | Self::Control(b) => buf.push(*b),
            Self::Tab => buf.push(b'\t'),
            Self::Newline => buf.push(b'\n'),
        }
    }

    /// Convert to char if possible (best effort for display/search)
    #[must_use]
    pub const fn to_char_lossy(&self) -> char {
        match self {
            Self::Unicode(c) => *c,
            Self::Byte(_) => '\u{FFFD}',
            Self::Tab => '\t',
            Self::Newline => '\n',
            Self::Control(b) => *b as char,
        }
    }
}

impl Display for Character {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.render(f)
    }
}

impl From<char> for Character {
    fn from(c: char) -> Self {
        match c {
            '\t' => Self::Tab,
            '\n' => Self::Newline,
            c if c.is_control() => Self::Control(c as u8),
            c => Self::Unicode(c),
        }
    }
}

impl From<u8> for Character {
    fn from(b: u8) -> Self {
        if b.is_ascii() {
            Self::from(b as char)
        } else {
            Self::Byte(b)
        }
    }
}

#[cfg(test)]
mod tests;
