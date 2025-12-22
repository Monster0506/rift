//! Command dispatch and keybindings
//! Translates keys into editor commands based on current mode

use crate::key::Key;
use crate::mode::Mode;
use crate::buffer::GapBuffer;

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
    DeleteChar,
    DeleteLine,
    InsertChar,
    InsertNewline,
    Backspace,

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
                b'x' => Command::DeleteChar,
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
                    Command::InsertChar
                } else {
                    Command::Noop
                }
            }
            Key::Ctrl(_) => {
                // Handle Ctrl key combinations in insert mode
                // For now, we'll insert them as characters (Ctrl+A = 0x01, etc.)
                Command::InsertChar
            }
            Key::Backspace => Command::Backspace,
            Key::Enter => Command::InsertNewline,
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

/// Execute a command on the editor buffer
pub fn execute_command(cmd: Command, buf: &mut GapBuffer, key: Option<Key>) {
    match cmd {
        Command::MoveLeft => {
            buf.move_left();
        }
        Command::MoveRight => {
            buf.move_right();
        }
        Command::MoveUp => {
            buf.move_up();
        }
        Command::MoveDown => {
            buf.move_down();
        }
        Command::MoveToLineStart => {
            buf.move_to_line_start();
        }
        Command::MoveToLineEnd => {
            buf.move_to_line_end();
        }
        Command::MoveToBufferStart => {
            // Move to start of buffer
            while buf.move_left() {}
        }
        Command::MoveToBufferEnd => {
            // Move to end of buffer
            while buf.move_right() {}
        }
        Command::DeleteChar => {
            buf.delete_forward();
        }
        Command::DeleteLine => {
            // TODO: Implement delete_line
        }
        Command::InsertChar => {
            if let Some(Key::Char(ch)) = key {
                let _ = buf.insert(ch);
            } else if let Some(Key::Ctrl(ch)) = key {
                // Insert Ctrl character (e.g., Ctrl+A = 0x01)
                let ctrl_char = if ch >= b'a' && ch <= b'z' {
                    ch - b'a' + 1
                } else {
                    ch
                };
                let _ = buf.insert(ctrl_char);
            }
        }
        Command::InsertNewline => {
            let _ = buf.insert(b'\n');
        }
        Command::Backspace => {
            buf.delete_backward();
        }
        Command::EnterInsertMode | Command::EnterInsertModeAfter => {
            // Mode change handled by editor
        }
        Command::Quit => {
            // Quit handled by editor
        }
        Command::Noop => {}
    }
}

