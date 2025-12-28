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
pub enum Granularity {
    Character,
    Word,
    Line,
    Page,
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
        // Handle control codes if needed?
        // Current Insert mode logic handled Ctrl+Char -> InsertByte.
        // We can replicate that if desired, or leave it to specific handlers.
        // For "shared input", usually we only care about text.
        // But let's support the existing behavior:
        Key::Ctrl(ch) => {
            // Handle ctrl logic if we want to emulate existing insert mode behavior
            // "Ctrl+A" -> '\u{1}'
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
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
