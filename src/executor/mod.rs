//! Command executor
//! Executes editor commands on the buffer

use crate::command::Command;
use crate::buffer::GapBuffer;

/// Execute a command on the editor buffer
pub fn execute_command(cmd: Command, buf: &mut GapBuffer) {
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
        Command::DeleteForward => {
            buf.delete_forward();
        }
        Command::DeleteBackward => {
            buf.delete_backward();
        }
        Command::DeleteLine => {
            // TODO: Implement delete_line
        }
        Command::InsertByte(b) => {
            let _ = buf.insert(b);
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

