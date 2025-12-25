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
        match key {
            Key::Char(ch) => {
                if ch == b'\t' || (32..127).contains(&ch) {
                    Command::InsertByte(ch)
                } else {
                    Command::Noop
                }
            }
            Key::Ctrl(ch) => {
                // Handle Ctrl key combinations in insert mode
                // Convert to control character (Ctrl+A = 0x01, etc.)
                let ctrl_char = if ch.is_ascii_lowercase() {
                    ch - b'a' + 1
                } else {
                    ch
                };
                Command::InsertByte(ctrl_char)
            }
            Key::Backspace => Command::DeleteBackward,
            Key::Enter => Command::InsertByte(b'\n'),
            Key::Tab => Command::InsertByte(b'\t'),
            Key::Escape => Command::EnterInsertMode, // Exit insert mode (returns to normal)
            _ => Command::Noop,
        }
    }

    fn translate_command_mode(&self, key: Key) -> Command {
        match key {
            Key::Char(ch) => {
                // Allow printable ASCII characters
                if (32..127).contains(&ch) {
                    Command::AppendToCommandLine(ch)
                } else {
                    Command::Noop
                }
            }
            Key::Backspace => Command::DeleteFromCommandLine,
            Key::Enter => Command::ExecuteCommandLine,
            Key::Escape => Command::Noop, // Exit handled by key handler
            _ => Command::Noop,
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

