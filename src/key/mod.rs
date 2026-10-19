//! Key representation for editor input

mod notation;
mod vt100;

pub use notation::{format_key, parse_key, parse_key_sequence};

use std::ops::BitOr;

/// Modifier bitset carried by a [`Key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const SHIFT: Mods = Mods(1);
    pub const CTRL: Mods = Mods(2);
    pub const ALT: Mods = Mods(4);

    #[must_use]
    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn without(self, other: Mods) -> Mods {
        Mods(self.0 & !other.0)
    }
}

impl BitOr for Mods {
    type Output = Mods;

    fn bitor(self, rhs: Mods) -> Mods {
        Mods(self.0 | rhs.0)
    }
}

/// The key itself, independent of modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// A key press. Build through [`Key::new`] (or the helpers) so the canonical
/// form holds: `Char` carries SHIFT only for Space or alongside CTRL/ALT,
/// CTRL/ALT letters are lowercase, and Ctrl+I/M/[ are Tab/Enter/Esc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub mods: Mods,
}

impl Key {
    pub const ENTER: Key = Key::plain(KeyCode::Enter);
    pub const ESC: Key = Key::plain(KeyCode::Esc);
    pub const TAB: Key = Key::plain(KeyCode::Tab);
    pub const BACKSPACE: Key = Key::plain(KeyCode::Backspace);
    pub const DELETE: Key = Key::plain(KeyCode::Delete);
    pub const INSERT: Key = Key::plain(KeyCode::Insert);
    pub const HOME: Key = Key::plain(KeyCode::Home);
    pub const END: Key = Key::plain(KeyCode::End);
    pub const PAGE_UP: Key = Key::plain(KeyCode::PageUp);
    pub const PAGE_DOWN: Key = Key::plain(KeyCode::PageDown);
    pub const UP: Key = Key::plain(KeyCode::Up);
    pub const DOWN: Key = Key::plain(KeyCode::Down);
    pub const LEFT: Key = Key::plain(KeyCode::Left);
    pub const RIGHT: Key = Key::plain(KeyCode::Right);
    pub const SPACE: Key = Key::char(' ');
    pub const SHIFT_TAB: Key = Key {
        code: KeyCode::Tab,
        mods: Mods::SHIFT,
    };
    pub const SHIFT_SPACE: Key = Key {
        code: KeyCode::Char(' '),
        mods: Mods::SHIFT,
    };

    const fn plain(code: KeyCode) -> Key {
        Key {
            code,
            mods: Mods::NONE,
        }
    }

    /// Plain character key, no modifiers.
    #[must_use]
    pub const fn char(c: char) -> Key {
        Key::plain(KeyCode::Char(c))
    }

    #[must_use]
    pub fn ctrl(c: char) -> Key {
        Key::new(KeyCode::Char(c), Mods::CTRL)
    }

    #[must_use]
    pub fn alt(c: char) -> Key {
        Key::new(KeyCode::Char(c), Mods::ALT)
    }

    #[must_use]
    pub fn f(n: u8) -> Key {
        Key::plain(KeyCode::F(n))
    }

    /// Build a key in canonical form.
    #[must_use]
    pub fn new(code: KeyCode, mods: Mods) -> Key {
        let KeyCode::Char(c) = code else {
            return Key { code, mods };
        };

        let held = mods.contains(Mods::CTRL) || mods.contains(Mods::ALT);
        if !held {
            if mods.contains(Mods::SHIFT) && c.is_ascii_lowercase() {
                return Key::char(c.to_ascii_uppercase());
            }
            let mods = if c == ' ' { mods } else { Mods::NONE };
            return Key { code, mods };
        }

        let c = c.to_ascii_lowercase();
        if mods == Mods::CTRL {
            match c {
                'i' => return Key::TAB,
                'm' => return Key::ENTER,
                '[' => return Key::ESC,
                _ => {}
            }
        }
        Key {
            code: KeyCode::Char(c),
            mods,
        }
    }

    /// The same key with extra modifiers, re-canonicalized.
    #[must_use]
    pub fn with(self, mods: Mods) -> Key {
        Key::new(self.code, self.mods | mods)
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
    pub fn fallback(&self) -> Option<Key> {
        let stripped = match self.code {
            KeyCode::Char(_) => Key {
                code: self.code,
                mods: self.mods.without(Mods::SHIFT),
            },
            code => Key::plain(code),
        };
        (stripped != *self).then_some(stripped)
    }

    /// Whether this key is `base`, or `base` plus modifiers that carry no
    /// text meaning (the same equivalence keymap lookup falls back on).
    #[must_use]
    pub fn is(&self, base: Key) -> bool {
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
        InputEvent::Key(key)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
