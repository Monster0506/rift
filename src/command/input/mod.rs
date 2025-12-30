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
/// Logical unit used to describe cursor movement, selection, and edit scope.
///
/// A `Granularity` represents *how much* content an operation applies to,
/// independent of direction. It is used by commands such as move, select,
/// delete, and change to express intent without encoding UI- or
/// representation-specific behavior.
///
/// Granularities are ordered conceptually from finest (`Character`) to
/// coarsest (`Document`), but no ordering is implied by the enum itself.
/// Implementations may choose appropriate semantics as long as the unit
/// boundaries are stable and intuitive.
pub enum Granularity {
    /// A single Unicode scalar value (code point).
    ///
    /// This is the smallest addressable unit in the buffer model. It does not
    /// correspond to grapheme clusters; combined characters, emoji sequences,
    /// and other multi-code-point constructs count as multiple characters.
    Character,

    /// A contiguous sequence of non-separator characters.
    ///
    /// Word boundaries are implementation-defined, but typically follow
    /// Unicode word boundary rules or editor conventions such as splitting on
    /// whitespace and punctuation.
    Word,

    /// A sentence of natural language text.
    ///
    /// Sentence boundaries are implementation-defined and commonly detected
    /// using punctuation such as '.', '!', or '?' followed by whitespace.
    /// This granularity is primarily intended for prose-oriented editing.
    Sentence,

    /// A logical line of text.
    ///
    /// Lines are delimited by newline characters in the underlying buffer and
    /// do not account for soft wrapping or visual layout.
    Line,

    /// A paragraph of text.
    ///
    /// Paragraphs are typically delimited by one or more blank lines. This
    /// granularity represents a higher-level structural unit than `Line` and
    /// is useful for both prose and code editing.
    Paragraph,

    /// A page-sized region of the document.
    ///
    /// The exact size of a page is implementation-defined and commonly maps to
    /// a viewport-sized vertical movement, similar to Page Up and Page Down.
    Page,

    /// The entire document.
    ///
    /// This granularity spans from the start of the buffer to the end and is
    /// used for operations such as moving to the beginning or end of the file
    /// or selecting all content.
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

        Key::CtrlHome => Some(InputIntent::Move(Direction::Left, Granularity::Document)),
        Key::CtrlEnd => Some(InputIntent::Move(Direction::Right, Granularity::Document)),

        Key::Resize(_, _) => None,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
