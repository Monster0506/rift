//! Key representation for editor input

mod notation;
mod vt100;

pub use notation::{format_key, parse_key, parse_key_sequence};

use std::ops::BitOr;

/// Modifier bitset carried by a [`Key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1);
    pub const CTRL: Self = Self(2);
    pub const ALT: Self = Self(4);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl BitOr for Mods {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// The key itself, independent of modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyCode {
    Char(char),
    F(u8),
    Insert,
    Enter,
    Esc,
    Tab,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
}

/// A key press.
///
/// Built through [`Key::new`] (or the helpers) so the canonical form holds: `Char` carries SHIFT only for Space or alongside CTRL/ALT, CTRL/ALT letters are lowercase, and Ctrl+I/M/[ are Tab/Enter/Esc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub mods: Mods,
}

impl Key {
    pub const ENTER: Self = Self::plain(KeyCode::Enter);
    pub const ESC: Self = Self::plain(KeyCode::Esc);
    pub const TAB: Self = Self::plain(KeyCode::Tab);
    pub const BACKSPACE: Self = Self::plain(KeyCode::Backspace);
    pub const DELETE: Self = Self::plain(KeyCode::Delete);
    pub const INSERT: Self = Self::plain(KeyCode::Insert);
    pub const HOME: Self = Self::plain(KeyCode::Home);
    pub const END: Self = Self::plain(KeyCode::End);
    pub const PAGE_UP: Self = Self::plain(KeyCode::PageUp);
    pub const PAGE_DOWN: Self = Self::plain(KeyCode::PageDown);
    pub const UP: Self = Self::plain(KeyCode::Up);
    pub const DOWN: Self = Self::plain(KeyCode::Down);
    pub const LEFT: Self = Self::plain(KeyCode::Left);
    pub const RIGHT: Self = Self::plain(KeyCode::Right);
    pub const SPACE: Self = Self::char(' ');
    pub const SHIFT_TAB: Self = Self {
        code: KeyCode::Tab,
        mods: Mods::SHIFT,
    };
    pub const SHIFT_SPACE: Self = Self {
        code: KeyCode::Char(' '),
        mods: Mods::SHIFT,
    };

    const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            mods: Mods::NONE,
        }
    }

    /// Plain character key, no modifiers.
    #[must_use]
    pub const fn char(c: char) -> Self {
        Self::plain(KeyCode::Char(c))
    }

    #[must_use]
    pub fn ctrl(c: char) -> Self {
        Self::new(KeyCode::Char(c), Mods::CTRL)
    }

    #[must_use]
    pub fn alt(c: char) -> Self {
        Self::new(KeyCode::Char(c), Mods::ALT)
    }

    #[must_use]
    pub const fn f(n: u8) -> Self {
        Self::plain(KeyCode::F(n))
    }

    /// Build a key in canonical form.
    #[must_use]
    pub fn new(code: KeyCode, mods: Mods) -> Self {
        let KeyCode::Char(c) = code else {
            return Self { code, mods };
        };

        let held = mods.contains(Mods::CTRL) || mods.contains(Mods::ALT);
        if !held {
            if mods.contains(Mods::SHIFT) && c.is_ascii_lowercase() {
                return Self::char(c.to_ascii_uppercase());
            }
            let mods = if c == ' ' { mods } else { Mods::NONE };
            return Self { code, mods };
        }

        let c = c.to_ascii_lowercase();
        if mods == Mods::CTRL {
            match c {
                'i' => return Self::TAB,
                'm' => return Self::ENTER,
                '[' => return Self::ESC,
                _ => {}
            }
        }
        Self {
            code: KeyCode::Char(c),
            mods,
        }
    }

    /// The same key with extra modifiers, re-canonicalized.
    #[must_use]
    pub fn with(self, mods: Mods) -> Self {
        Self::new(self.code, self.mods | mods)
    }

    /// The character this key types, if it types one.
    #[must_use]
    pub fn text(&self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if self.mods.without(Mods::SHIFT).is_empty() && !c.is_control() => {
                Some(c)
            }
            _ => None,
        }
    }

    /// The key with modifiers that carry no text meaning removed (SHIFT on
    /// characters, every modifier on non-character keys). `None` when
    /// nothing would change.
    #[must_use]
    pub fn fallback(&self) -> Option<Self> {
        let stripped = match self.code {
            KeyCode::Char(_) => Self {
                code: self.code,
                mods: self.mods.without(Mods::SHIFT),
            },
            code => Self::plain(code),
        };
        (stripped != *self).then_some(stripped)
    }

    /// Whether this key is `base`, or `base` plus modifiers that carry no
    /// text meaning (the same equivalence keymap lookup falls back on).
    #[must_use]
    pub fn is(&self, base: Self) -> bool {
        *self == base || self.fallback() == Some(base)
    }

    /// Convert key to xterm byte sequence for PTY input.
    #[must_use]
    pub fn to_vt100_bytes(&self) -> Vec<u8> {
        vt100::encode(*self)
    }
}

/// Everything the terminal can deliver to the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Key(Key),
    Resize(u16, u16),
    /// A terminal bracketed-paste block: the whole clipboard payload as one
    /// event, so it never gets decomposed into individual command keys.
    Paste(String),
}

impl From<Key> for InputEvent {
    fn from(key: Key) -> Self {
        Self::Key(key)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
