use std::fmt::{self, Display, Formatter};
use unicode_width::UnicodeWidthChar;

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
    pub fn render(&self, out: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Character::Unicode(c) => write!(out, "{}", c),
            Character::Byte(b) => write!(out, "\\x{:02X}", b),
            Character::Tab => write!(out, "\t"),
            Character::Newline => writeln!(out),
            Character::Control(b) => {
                // Control chars are usually 0x00-0x1F. We map them to ^@, ^A, etc.
                // 0 -> @ (64)
                // 1 -> A (65)
                write!(out, "^{}", (b + 64) as char)
            }
        }
    }

    /// Get the visual width of the character at a specific column
    pub fn render_width(&self, col: usize, tab_width: usize) -> usize {
        match self {
            Character::Unicode(c) => UnicodeWidthChar::width(*c).unwrap_or(0),
            Character::Byte(_) => 4, // \xNN is 4 chars
            Character::Tab => tab_width - (col % tab_width),
            Character::Newline => 0,    // usually implied
            Character::Control(_) => 2, // ^C is 2 chars
        }
    }

    /// Get the logical width (unit of movement)
    pub fn logical_width(&self) -> usize {
        1
    }

    /// Get the UTF-8 byte length for serialization
    pub fn len_utf8(&self) -> usize {
        match self {
            Character::Unicode(c) => c.len_utf8(),
            Character::Byte(_) => 1,
            Character::Tab => 1,
            Character::Newline => 1,
            Character::Control(_) => 1,
        }
    }

    /// Encode the character as UTF-8 bytes into a buffer
    pub fn encode_utf8(&self, buf: &mut Vec<u8>) {
        match self {
            Character::Unicode(c) => {
                let mut b = [0; 4];
                let s = c.encode_utf8(&mut b);
                buf.extend_from_slice(s.as_bytes());
            }
            Character::Byte(b) => buf.push(*b),
            Character::Tab => buf.push(b'\t'),
            Character::Newline => buf.push(b'\n'),
            Character::Control(b) => buf.push(*b),
        }
    }

    /// Convert to char if possible (best effort for display/search)
    pub fn to_char_lossy(&self) -> char {
        match self {
            Character::Unicode(c) => *c,
            Character::Byte(_) => '\u{FFFD}', // Replacement char
            Character::Tab => '\t',
            Character::Newline => '\n',
            Character::Control(b) => *b as char,
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
            '\t' => Character::Tab,
            '\n' => Character::Newline,
            c if c.is_control() => Character::Control(c as u8),
            c => Character::Unicode(c),
        }
    }
}

impl From<u8> for Character {
    fn from(b: u8) -> Self {
        if b.is_ascii() {
            Character::from(b as char)
        } else {
            Character::Byte(b)
        }
    }
}

#[cfg(test)]
#[path = "character_tests.rs"]
mod tests;
