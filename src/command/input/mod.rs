//! Shared input handling logic
//! translates raw keys into abstract input intents (Type, Move, Delete, etc.)

use crate::key::Key;

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
    match key {
        Key::Char(ch) => {
            // Handle printable characters and Tab
            if ch == '\t' || !ch.is_control() {
                Some(InputIntent::Type(ch))
            } else {
                None
            }
        }
        Key::Ctrl(ch) => {
            let ctrl_char = if ch.is_ascii_lowercase() {
                (ch - b'a' + 1) as char
            } else if ch.is_ascii_uppercase() {
                (ch - b'A' + 1) as char
            } else {
                ch as char
            };
            Some(InputIntent::Type(ctrl_char))
        }

        Key::Enter => Some(InputIntent::Accept),
        Key::Tab => Some(InputIntent::Type('\t')), // Treat tab as input char for now
        Key::Escape => Some(InputIntent::Cancel),

        // Deletion
        Key::Backspace => Some(InputIntent::Delete(Direction::Left, Granularity::Character)),
        Key::Delete => Some(InputIntent::Delete(
            Direction::Right,
            Granularity::Character,
        )),

        // Navigation
        Key::ArrowLeft => Some(InputIntent::Move(Direction::Left, Granularity::Character)),
        Key::ArrowRight => Some(InputIntent::Move(Direction::Right, Granularity::Character)),
        Key::ArrowUp => Some(InputIntent::Move(Direction::Up, Granularity::Character)),
        Key::ArrowDown => Some(InputIntent::Move(Direction::Down, Granularity::Character)),

        Key::CtrlArrowLeft => Some(InputIntent::Move(Direction::Left, Granularity::Word)),
        Key::CtrlArrowRight => Some(InputIntent::Move(Direction::Right, Granularity::Word)),
        Key::CtrlArrowUp => Some(InputIntent::Move(Direction::Up, Granularity::Word)), // Less common but consistent
        Key::CtrlArrowDown => Some(InputIntent::Move(Direction::Down, Granularity::Word)),

        Key::Home => Some(InputIntent::Move(Direction::Left, Granularity::Line)),
        Key::End => Some(InputIntent::Move(Direction::Right, Granularity::Line)),
        Key::PageUp => Some(InputIntent::Move(Direction::Up, Granularity::Page)),
        Key::PageDown => Some(InputIntent::Move(Direction::Down, Granularity::Page)),

        Key::CtrlHome => Some(InputIntent::Move(Direction::Left, Granularity::Document)),
        Key::CtrlEnd => Some(InputIntent::Move(Direction::Right, Granularity::Document)),

        Key::Alt(_) => None, // handled by keymap
        Key::Resize(_, _) => None,
        Key::ShiftTab => None, // handled before resolve_input in command mode
        Key::ShiftSpace => None, // Visual-mode-only; handled by keymap
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
