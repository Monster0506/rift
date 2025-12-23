//! Command executor
//! Executes editor commands on the buffer

use crate::command::Command;
use crate::key::Key;
use crate::buffer::GapBuffer;

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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

