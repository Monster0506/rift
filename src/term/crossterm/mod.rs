//! Crossterm-based terminal backend
//! Cross-platform terminal operations using crossterm

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{stdout, Write};

use crate::color::Color;
use crate::key::Key;
use crate::term::{ColorTerminal, Size, TerminalBackend};

/// Crossterm-based terminal backend implementation
pub struct CrosstermBackend {
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
}

impl CrosstermBackend {
    pub fn new() -> Result<Self, String> {
        Ok(CrosstermBackend {
            raw_mode_enabled: false,
            alternate_screen_enabled: false,
        })
    }
}

impl TerminalBackend for CrosstermBackend {
    fn init(&mut self) -> Result<(), String> {
        // Enable alternate screen buffer (prevents scrolling in main buffer)
        execute!(stdout(), terminal::EnterAlternateScreen)
            .map_err(|e| format!("Failed to enter alternate screen: {e}"))?;
        self.alternate_screen_enabled = true;

        // Enable raw mode
        terminal::enable_raw_mode().map_err(|e| format!("Failed to enable raw mode: {e}"))?;
        self.raw_mode_enabled = true;

        // Hide cursor during rendering
        execute!(stdout(), cursor::Hide).map_err(|e| format!("Failed to hide cursor: {e}"))?;

        Ok(())
    }

    fn deinit(&mut self) {
        // Show cursor before exiting
        let _ = execute!(stdout(), cursor::Show);

        if self.raw_mode_enabled {
            let _ = terminal::disable_raw_mode();
            self.raw_mode_enabled = false;
        }

        // Exit alternate screen buffer
        if self.alternate_screen_enabled {
            let _ = execute!(stdout(), terminal::LeaveAlternateScreen);
            self.alternate_screen_enabled = false;
        }
    }

    fn read_key(&mut self) -> Result<Key, String> {
        loop {
            if let Event::Key(key_event) =
                event::read().map_err(|e| format!("Failed to read event: {e}"))?
            {
                if key_event.kind == event::KeyEventKind::Press {
                    return Ok(translate_key_event(key_event));
                }
                // Ignore key releases
            } else {
                // Ignore other events
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        stdout()
            .write_all(bytes)
            .map_err(|e| format!("Write failed: {e}"))?;
        stdout().flush().map_err(|e| format!("Flush failed: {e}"))?;
        Ok(())
    }

    fn get_size(&self) -> Result<Size, String> {
        let (cols, rows) =
            terminal::size().map_err(|e| format!("Failed to get terminal size: {e}"))?;
        Ok(Size { rows, cols })
    }

    fn clear_screen(&mut self) -> Result<(), String> {
        execute!(stdout(), terminal::Clear(ClearType::All))
            .map_err(|e| format!("Failed to clear screen: {e}"))?;
        execute!(stdout(), cursor::MoveTo(0, 0))
            .map_err(|e| format!("Failed to move cursor: {e}"))?;
        Ok(())
    }

    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String> {
        execute!(stdout(), cursor::MoveTo(col, row))
            .map_err(|e| format!("Failed to move cursor: {e}"))?;
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), String> {
        execute!(stdout(), cursor::Hide).map_err(|e| format!("Failed to hide cursor: {e}"))?;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), String> {
        execute!(stdout(), cursor::Show).map_err(|e| format!("Failed to show cursor: {e}"))?;
        Ok(())
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        execute!(stdout(), terminal::Clear(ClearType::UntilNewLine))
            .map_err(|e| format!("Failed to clear to end of line: {e}"))?;
        Ok(())
    }
}

impl ColorTerminal for CrosstermBackend {
    fn set_foreground_color(&mut self, color: Color) -> Result<(), String> {
        execute!(stdout(), SetForegroundColor(color.to_crossterm()))
            .map_err(|e| format!("Failed to set foreground color: {e}"))?;
        Ok(())
    }

    fn set_background_color(&mut self, color: Color) -> Result<(), String> {
        execute!(stdout(), SetBackgroundColor(color.to_crossterm()))
            .map_err(|e| format!("Failed to set background color: {e}"))?;
        Ok(())
    }

    fn reset_colors(&mut self) -> Result<(), String> {
        execute!(stdout(), ResetColor).map_err(|e| format!("Failed to reset colors: {e}"))?;
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
                Key::Char(ch as u8)
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
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Delete => Key::Delete,
        _ => Key::Char(0), // Unknown key
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
