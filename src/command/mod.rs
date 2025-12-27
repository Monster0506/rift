//! Command dispatch and keybindings
//! Translates keys into editor commands based on current mode

/// ## command/ Invariants
///
/// - `Command` represents editor-level intent, not key-level input.
/// - Commands contain no terminal- or platform-specific concepts.
/// - All data required to apply a command is contained within the command.
/// - Commands are immutable once created.
/// - Adding a new command requires explicit executor support.
use crate::key::Key;
use crate::mode::Mode;

pub mod input;

/// Editor commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    // Movement
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveToLineStart,
    MoveToLineEnd,
    MoveToBufferStart,
    MoveToBufferEnd,

    // Editing
    EnterInsertMode,
    EnterInsertModeAfter,
    DeleteForward,
    DeleteBackward,
    DeleteLine,
    InsertByte(u8),

    // Mode transitions
    EnterCommandMode,

    // Command line editing
    AppendToCommandLine(u8),
    DeleteFromCommandLine,
    ExecuteCommandLine,

    // Control
    Quit,
    Noop,
}

impl Command {
    /// Check if command mutates the buffer content
    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            Command::DeleteForward
                | Command::DeleteBackward
                | Command::DeleteLine
                | Command::InsertByte(_)
        )
    }
}

/// Command dispatcher state
pub struct Dispatcher {
    mode: Mode,
    pending_key: Option<Key>,
}

impl Dispatcher {
    #[must_use]
    pub fn new(mode: Mode) -> Self {
        Dispatcher {
            mode,
            pending_key: None,
        }
    }

    /// Translate a key into a command based on current mode
    pub fn translate_key(&mut self, key: Key) -> Command {
        match self.mode {
            Mode::Normal => self.translate_normal_mode(key),
            Mode::Insert => self.translate_insert_mode(key),
            Mode::Command => self.translate_command_mode(key),
        }
    }

    fn translate_normal_mode(&mut self, key: Key) -> Command {
        // Handle multi-key sequences
        if let Some(pending) = self.pending_key.take() {
            return self.handle_normal_mode_sequence(pending, key);
        }

        match key {
            Key::Char(ch) => match ch {
                b'h' => Command::MoveLeft,
                b'j' => Command::MoveDown,
                b'k' => Command::MoveUp,
                b'l' => Command::MoveRight,
                b'0' => Command::MoveToLineStart,
                b'$' => Command::MoveToLineEnd,
                b'i' => Command::EnterInsertMode,
                b'a' => Command::EnterInsertModeAfter,
                b'x' => Command::DeleteForward,
                b'q' => Command::Quit,
                b':' => Command::EnterCommandMode,
                b'd' => {
                    // Start sequence for 'dd'
                    self.pending_key = Some(key);
                    Command::Noop
                }
                b'g' => {
                    // Start sequence for 'gg'
                    self.pending_key = Some(key);
                    Command::Noop
                }
                b'G' => Command::MoveToBufferEnd,
                _ => Command::Noop,
            },
            Key::ArrowLeft => Command::MoveLeft,
            Key::ArrowRight => Command::MoveRight,
            Key::ArrowUp => Command::MoveUp,
            Key::ArrowDown => Command::MoveDown,
            Key::Home => Command::MoveToLineStart,
            Key::End => Command::MoveToLineEnd,
            _ => Command::Noop,
        }
    }

    fn handle_normal_mode_sequence(&mut self, first: Key, second: Key) -> Command {
        match (first, second) {
            (Key::Char(b'd'), Key::Char(b'd')) => Command::DeleteLine,
            (Key::Char(b'g'), Key::Char(b'g')) => Command::MoveToBufferStart,
            _ => Command::Noop,
        }
    }

    fn translate_insert_mode(&self, key: Key) -> Command {
        use self::input::{Direction, InputIntent};

        // Resolve shared input intent
        if let Some(intent) = input::resolve_input(key) {
            match intent {
                InputIntent::Type(ch) => {
                    // Filter out non-byte characters if necessary, or assume char fits in u8 for now
                    // as InsertByte takes u8. Using ch as u8 only works for ASCII.
                    if ch.is_ascii() {
                        Command::InsertByte(ch as u8)
                    } else {
                        Command::Noop
                    }
                }
                // TODO: Implement granular movement
                // For now, fall back to character movement so keys aren't dead
                InputIntent::Move(dir, _) => match dir {
                    Direction::Left => Command::MoveLeft,
                    Direction::Right => Command::MoveRight,
                    Direction::Up => Command::MoveUp,
                    Direction::Down => Command::MoveDown,
                },
                InputIntent::Delete(Direction::Left, _) => Command::DeleteBackward, // Backspace
                InputIntent::Delete(Direction::Right, _) => Command::DeleteForward, // Delete
                InputIntent::Delete(_, _) => Command::Noop, // Other deletes not supported yet
                InputIntent::Accept => Command::InsertByte(b'\n'),
                InputIntent::Cancel => Command::EnterInsertMode, // Toggle back to normal
            }
        } else {
            Command::Noop
        }
    }

    fn translate_command_mode(&self, key: Key) -> Command {
        use self::input::{Direction, Granularity, InputIntent};

        if let Some(intent) = input::resolve_input(key) {
            match intent {
                InputIntent::Type(ch) => {
                    if ch.is_ascii() {
                        Command::AppendToCommandLine(ch as u8)
                    } else {
                        Command::Noop
                    }
                }
                InputIntent::Move(dir, Granularity::Line) => match dir {
                    Direction::Left => Command::MoveToLineStart,
                    Direction::Right => Command::MoveToLineEnd,
                    _ => Command::Noop,
                },
                InputIntent::Move(dir, Granularity::Word) => {
                    // TODO: Implement word-wise movement for command line
                    // For now, fall back to character movement
                    match dir {
                        Direction::Left => Command::MoveLeft,
                        Direction::Right => Command::MoveRight,
                        Direction::Up => Command::MoveUp,
                        Direction::Down => Command::MoveDown,
                    }
                }
                InputIntent::Move(dir, _) => match dir {
                    Direction::Left => Command::MoveLeft,
                    Direction::Right => Command::MoveRight,
                    Direction::Up => Command::MoveUp,
                    Direction::Down => Command::MoveDown,
                },
                InputIntent::Delete(Direction::Left, _) => Command::DeleteFromCommandLine, // Backspace
                InputIntent::Delete(Direction::Right, _) => Command::DeleteForward, // Forward delete
                InputIntent::Delete(_, _) => Command::Noop,
                InputIntent::Accept => Command::ExecuteCommandLine,
                InputIntent::Cancel => Command::Noop, // Handled by KeyHandler
            }
        } else {
            Command::Noop
        }
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        // Clear pending key when switching modes
        self.pending_key = None;
    }

    #[must_use]
    pub fn pending_key(&self) -> Option<Key> {
        self.pending_key
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
