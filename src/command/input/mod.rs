//! Shared input handling logic
//! translates raw keys into abstract input intents (Type, Move, Delete, etc.)

use crate::key::{Key, KeyCode, Mods};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How much content move/select/delete/change applies to, independent of
/// direction. Ordered conceptually finest (`Character`) to coarsest (`Document`).
pub enum Granularity {
    /// A single Unicode scalar value; does not correspond to grapheme
    /// clusters, so combined characters count as multiple.
    Character,

    /// A contiguous run of non-separator characters (word-boundary rules
    /// are implementation-defined).
    Word,

    /// A natural-language sentence, boundaries typically detected via
    /// '.', '!', or '?' followed by whitespace.
    Sentence,

    /// A logical (newline-delimited) line; does not account for soft wrapping.
    Line,

    /// A paragraph, typically delimited by one or more blank lines.
    Paragraph,

    /// A viewport-sized vertical region, like Page Up/Page Down.
    Page,

    /// The entire document, start to end.
    Document,
}

/// Abstract intent for text input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputIntent {
    /// Type a character
    Type(char),
    /// Move cursor
    Move(Direction, Granularity),
    /// Delete content
    Delete(Direction, Granularity),
    /// Accept input (e.g. Enter)
    Accept,
    /// Cancel/Exit (e.g. Escape)
    Cancel,
}

/// Resolve a key into an input intent
pub fn resolve_input(key: Key) -> Option<InputIntent> {
    let ctrl = key.mods.contains(Mods::CTRL);
    let move_by = |direction, plain, ctrl_unit| {
        let unit = if ctrl { ctrl_unit } else { plain };
        Some(InputIntent::Move(direction, unit))
    };

    match key.code {
        KeyCode::Char(ch) => resolve_char(key, ch),
        KeyCode::Enter => Some(InputIntent::Accept),
        // Shift+Tab is handled before resolve_input in command mode
        KeyCode::Tab if key.mods.contains(Mods::SHIFT) => None,
        KeyCode::Tab => Some(InputIntent::Type('\t')),
        KeyCode::Esc => Some(InputIntent::Cancel),

        KeyCode::Backspace => Some(InputIntent::Delete(Direction::Left, Granularity::Character)),
        KeyCode::Delete => Some(InputIntent::Delete(
            Direction::Right,
            Granularity::Character,
        )),

        KeyCode::Left => move_by(Direction::Left, Granularity::Character, Granularity::Word),
        KeyCode::Right => move_by(Direction::Right, Granularity::Character, Granularity::Word),
        KeyCode::Up => move_by(Direction::Up, Granularity::Character, Granularity::Word),
        KeyCode::Down => move_by(Direction::Down, Granularity::Character, Granularity::Word),

        KeyCode::Home => move_by(Direction::Left, Granularity::Line, Granularity::Document),
        KeyCode::End => move_by(Direction::Right, Granularity::Line, Granularity::Document),
        KeyCode::PageUp => move_by(Direction::Up, Granularity::Page, Granularity::Page),
        KeyCode::PageDown => move_by(Direction::Down, Granularity::Page, Granularity::Page),

        KeyCode::Insert | KeyCode::F(_) => None,
    }
}

fn resolve_char(key: Key, ch: char) -> Option<InputIntent> {
    if let Some(text) = key.text() {
        return Some(InputIntent::Type(text));
    }
    if key.mods != Mods::CTRL {
        return None; // handled by keymap
    }

    let ctrl_char = if ch.is_ascii_lowercase() {
        (ch as u8 - b'a' + 1) as char
    } else {
        ch
    };
    Some(InputIntent::Type(ctrl_char))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
