//! Command dispatch and keybindings
//! Translates keys into editor commands based on current mode

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
                if ch == b'\t' || (ch >= 32 && ch < 127) {
                    Command::InsertByte(ch)
                } else {
                    Command::Noop
                }
            }
            Key::Ctrl(ch) => {
                // Handle Ctrl key combinations in insert mode
                // Convert to control character (Ctrl+A = 0x01, etc.)
                let ctrl_char = if ch >= b'a' && ch <= b'z' {
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

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        // Clear pending key when switching modes
        self.pending_key = None;
    }

    pub fn pending_key(&self) -> Option<Key> {
        self.pending_key
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

