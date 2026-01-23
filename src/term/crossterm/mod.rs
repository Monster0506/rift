//! Crossterm-based terminal backend
//! Cross-platform terminal operations using crossterm

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{stdout, BufWriter, Write};

use crate::color::Color;
use crate::key::Key;
use crate::term::{ColorTerminal, Size, TerminalBackend};

/// Crossterm-based terminal backend implementation
pub struct CrosstermBackend {
    writer: BufWriter<std::io::Stdout>,
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
}

impl CrosstermBackend {
    pub fn new() -> Result<Self, String> {
        Ok(CrosstermBackend {
            writer: BufWriter::with_capacity(8192, stdout()),
            raw_mode_enabled: false,
            alternate_screen_enabled: false,
        })
    }
}

impl TerminalBackend for CrosstermBackend {
    fn init(&mut self) -> Result<(), String> {
        // Enable alternate screen buffer (prevents scrolling in main buffer)
        execute!(self.writer, terminal::EnterAlternateScreen)
            .map_err(|e| format!("Failed to enter alternate screen: {e}"))?;
        self.alternate_screen_enabled = true;

        // Enable raw mode
        terminal::enable_raw_mode().map_err(|e| format!("Failed to enable raw mode: {e}"))?;
        self.raw_mode_enabled = true;

        // Hide cursor during rendering
        execute!(self.writer, cursor::Hide).map_err(|e| format!("Failed to hide cursor: {e}"))?;

        self.writer
            .flush()
            .map_err(|e| format!("Failed to flush: {e}"))?;
        Ok(())
    }

    fn deinit(&mut self) {
        // Show cursor before exiting
        let _ = execute!(self.writer, cursor::Show);

        if self.raw_mode_enabled {
            let _ = terminal::disable_raw_mode();
            self.raw_mode_enabled = false;
        }

        // Exit alternate screen buffer
        if self.alternate_screen_enabled {
            let _ = execute!(self.writer, terminal::LeaveAlternateScreen);
            self.alternate_screen_enabled = false;
        }
        let _ = self.writer.flush();
    }

    fn poll(&mut self, duration: std::time::Duration) -> Result<bool, String> {
        event::poll(duration).map_err(|e| format!("Failed to poll event: {e}"))
    }

    fn read_key(&mut self) -> Result<Option<Key>, String> {
        match event::read().map_err(|e| format!("Failed to read event: {e}"))? {
            Event::Key(key_event) => {
                if key_event.kind == event::KeyEventKind::Press {
                    Ok(Some(translate_key_event(key_event)))
                } else {
                    Ok(None)
                }
            }
            Event::Resize(cols, rows) => Ok(Some(Key::Resize(cols, rows))),
            _ => Ok(None),
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(bytes)
            .map_err(|e| format!("Write failed: {e}"))?;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), String> {
        self.writer
            .flush()
            .map_err(|e| format!("Flush failed: {e}"))
    }

    fn get_size(&self) -> Result<Size, String> {
        let (cols, rows) =
            terminal::size().map_err(|e| format!("Failed to get terminal size: {e}"))?;
        Ok(Size { rows, cols })
    }

    fn clear_screen(&mut self) -> Result<(), String> {
        execute!(self.writer, terminal::Clear(ClearType::All))
            .map_err(|e| format!("Failed to clear screen: {e}"))?;
        execute!(self.writer, cursor::MoveTo(0, 0))
            .map_err(|e| format!("Failed to move cursor: {e}"))?;
        Ok(())
    }

    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String> {
        execute!(self.writer, cursor::MoveTo(col, row))
            .map_err(|e| format!("Failed to move cursor: {e}"))?;
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), String> {
        execute!(self.writer, cursor::Hide).map_err(|e| format!("Failed to hide cursor: {e}"))?;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), String> {
        execute!(self.writer, cursor::Show).map_err(|e| format!("Failed to show cursor: {e}"))?;
        Ok(())
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        execute!(self.writer, terminal::Clear(ClearType::UntilNewLine))
            .map_err(|e| format!("Failed to clear to end of line: {e}"))?;
        Ok(())
    }
}

impl ColorTerminal for CrosstermBackend {
    fn set_foreground_color(&mut self, color: Color) -> Result<(), String> {
        execute!(self.writer, SetForegroundColor(color.to_crossterm()))
            .map_err(|e| format!("Failed to set foreground color: {e}"))?;
        Ok(())
    }

    fn set_background_color(&mut self, color: Color) -> Result<(), String> {
        execute!(self.writer, SetBackgroundColor(color.to_crossterm()))
            .map_err(|e| format!("Failed to set background color: {e}"))?;
        Ok(())
    }

    fn reset_colors(&mut self) -> Result<(), String> {
        execute!(self.writer, ResetColor).map_err(|e| format!("Failed to reset colors: {e}"))?;
        Ok(())
    }
}

/// Translate crossterm `KeyEvent` to our Key enum
pub(crate) fn translate_key_event(key_event: KeyEvent) -> Key {
    let modifiers = key_event.modifiers;
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let _shift = modifiers.contains(KeyModifiers::SHIFT);
    let _alt = modifiers.contains(KeyModifiers::ALT);

    match key_event.code {
        KeyCode::Char(ch) => {
            // Handle Enter key that comes through as character (some terminals send '\r' or '\n')
            if ch == '\r' || ch == '\n' {
                return Key::Enter;
            }
            if ctrl {
                Key::Ctrl(ch as u8)
            } else {
                Key::Char(ch)
            }
        }
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Up => {
            if ctrl {
                Key::CtrlArrowUp
            } else {
                Key::ArrowUp
            }
        }
        KeyCode::Down => {
            if ctrl {
                Key::CtrlArrowDown
            } else {
                Key::ArrowDown
            }
        }
        KeyCode::Left => {
            if ctrl {
                Key::CtrlArrowLeft
            } else {
                Key::ArrowLeft
            }
        }
        KeyCode::Right => {
            if ctrl {
                Key::CtrlArrowRight
            } else {
                Key::ArrowRight
            }
        }
        KeyCode::Home => {
            if ctrl {
                Key::CtrlHome
            } else {
                Key::Home
            }
        }
        KeyCode::End => {
            if ctrl {
                Key::CtrlEnd
            } else {
                Key::End
            }
        }
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Delete => Key::Delete,
        _ => Key::Char('\0'), // Unknown key
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
