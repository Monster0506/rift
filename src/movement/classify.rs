//! Character classification for movement operations

use crate::character::Character;

/// Character categories for word movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CharClass {
    /// Whitespace characters (space, tab, newline, etc.)
    Whitespace,
    /// Alphanumeric characters and underscore
    Alphanumeric,
    /// Symbols and punctuation
    Symbol,
}

/// Classify a character for word boundary detection
#[must_use]
pub(crate) fn classify_char(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Alphanumeric
    } else {
        CharClass::Symbol
    }
}

/// Classify a Character (from buffer) for word boundary detection
#[must_use]
pub(crate) fn classify_character(ch: Character) -> CharClass {
    classify_char(ch.to_char_lossy())
}

/// Check if a character is part of a word (not whitespace)
#[must_use]
pub(crate) const fn is_word_char(c: char) -> bool {
    !c.is_whitespace()
}

/// Check if a character indicates sentence end
#[must_use]
pub(crate) const fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?')
}
