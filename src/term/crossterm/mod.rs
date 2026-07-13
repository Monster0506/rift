//! Crossterm-based terminal backend
//! Cross-platform terminal operations using crossterm

use crate::term::CursorShape;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::{Color as CrosstermColor, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{stdout, BufWriter, Write};

use crate::color::Color;
use crate::key::Key;
use crate::screen_buffer::{AttrKind, StyleOp};
use crate::term::{ColorTerminal, Size, TerminalBackend};

/// Convert the core `Color` type to crossterm's `Color`.
#[must_use]
pub fn color_to_crossterm(color: Color) -> CrosstermColor {
    match color {
        Color::Reset => CrosstermColor::Reset,
        Color::Black => CrosstermColor::Black,
        Color::DarkGrey => CrosstermColor::DarkGrey,
        Color::Red => CrosstermColor::Red,
        Color::DarkRed => CrosstermColor::DarkRed,
        Color::Green => CrosstermColor::Green,
        Color::DarkGreen => CrosstermColor::DarkGreen,
        Color::Yellow => CrosstermColor::Yellow,
        Color::DarkYellow => CrosstermColor::DarkYellow,
        Color::Blue => CrosstermColor::Blue,
        Color::DarkBlue => CrosstermColor::DarkBlue,
        Color::Magenta => CrosstermColor::Magenta,
        Color::DarkMagenta => CrosstermColor::DarkMagenta,
        Color::Cyan => CrosstermColor::Cyan,
        Color::DarkCyan => CrosstermColor::DarkCyan,
        Color::White => CrosstermColor::White,
        Color::Grey => CrosstermColor::Grey,
        Color::Ansi256(n) => CrosstermColor::AnsiValue(n),
        Color::Rgb { r, g, b } => CrosstermColor::Rgb { r, g, b },
    }
}

/// Convert crossterm's `Color` to the core `Color` type.
#[must_use]
pub fn color_from_crossterm(color: CrosstermColor) -> Color {
    match color {
        CrosstermColor::Reset => Color::Reset,
        CrosstermColor::Black => Color::Black,
        CrosstermColor::DarkGrey => Color::DarkGrey,
        CrosstermColor::Red => Color::Red,
        CrosstermColor::DarkRed => Color::DarkRed,
        CrosstermColor::Green => Color::Green,
        CrosstermColor::DarkGreen => Color::DarkGreen,
        CrosstermColor::Yellow => Color::Yellow,
        CrosstermColor::DarkYellow => Color::DarkYellow,
        CrosstermColor::Blue => Color::Blue,
        CrosstermColor::DarkBlue => Color::DarkBlue,
        CrosstermColor::Magenta => Color::Magenta,
        CrosstermColor::DarkMagenta => Color::DarkMagenta,
        CrosstermColor::Cyan => Color::Cyan,
        CrosstermColor::DarkCyan => Color::DarkCyan,
        CrosstermColor::White => Color::White,
        CrosstermColor::Grey => Color::Grey,
        CrosstermColor::AnsiValue(n) => Color::Ansi256(n),
        CrosstermColor::Rgb { r, g, b } => Color::Rgb { r, g, b },
    }
}

/// The crossterm `Attribute` for turning one text attribute on or off.
fn crossterm_attr(kind: AttrKind, on: bool) -> crossterm::style::Attribute {
    use crossterm::style::Attribute;
    match (kind, on) {
        (AttrKind::Bold, true) => Attribute::Bold,
        (AttrKind::Bold, false) => Attribute::NormalIntensity,
        (AttrKind::Italic, true) => Attribute::Italic,
        (AttrKind::Italic, false) => Attribute::NoItalic,
        (AttrKind::Underline, true) => Attribute::Underlined,
        (AttrKind::Underline, false) => Attribute::NoUnderline,
        (AttrKind::Strike, true) => Attribute::CrossedOut,
        (AttrKind::Strike, false) => Attribute::NotCrossedOut,
        (AttrKind::Reverse, true) => Attribute::Reverse,
        (AttrKind::Reverse, false) => Attribute::NoReverse,
    }
}

/// Serialize a terminal-agnostic `StyleOp` sequence into ANSI escape bytes.
/// The one place `screen_buffer`'s diff output meets crossterm's encoder.
pub fn encode_style_ops(ops: &[StyleOp], buf: &mut Vec<u8>) -> Result<(), String> {
    use crossterm::style::{Attribute, SetAttribute};

    for op in ops {
        match *op {
            StyleOp::ResetColor => {
                queue!(buf, ResetColor).map_err(|e| format!("Failed to reset colors: {e}"))?;
            }
            StyleOp::SetForeground(color) => {
                queue!(buf, SetForegroundColor(color_to_crossterm(color)))
                    .map_err(|e| format!("Failed to set fg: {e}"))?;
            }
            StyleOp::SetBackground(color) => {
                queue!(buf, SetBackgroundColor(color_to_crossterm(color)))
                    .map_err(|e| format!("Failed to set bg: {e}"))?;
            }
            StyleOp::SetAttr(kind, on) => {
                queue!(buf, SetAttribute(crossterm_attr(kind, on)))
                    .map_err(|e| format!("Failed to set attr: {e}"))?;
            }
            StyleOp::ResetAttrs => {
                queue!(buf, SetAttribute(Attribute::Reset))
                    .map_err(|e| format!("Failed to reset attrs: {e}"))?;
            }
        }
    }
    Ok(())
}

/// Crossterm-based terminal backend implementation, generic over its output
/// sink so the same ANSI encoding path can target a buffer instead of a TTY.
pub struct CrosstermBackend<W: Write = std::io::Stdout> {
    writer: BufWriter<W>,
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
}

impl CrosstermBackend<std::io::Stdout> {
    pub fn new() -> Result<Self, String> {
        Ok(CrosstermBackend {
            // Sized so a full colored frame flushes in one write syscall.
            writer: BufWriter::with_capacity(256 * 1024, stdout()),
            raw_mode_enabled: false,
            alternate_screen_enabled: false,
        })
    }
}

impl<W: Write> CrosstermBackend<W> {
    /// Build a backend that encodes through the same crossterm calls as
    /// `new()` but writes to `writer` instead of stdout (no TTY required).
    pub fn with_writer(writer: W) -> Self {
        CrosstermBackend {
            writer: BufWriter::with_capacity(256 * 1024, writer),
            raw_mode_enabled: false,
            alternate_screen_enabled: false,
        }
    }
}

// Ensures raw mode and the alternate screen are restored even if a fallible
// call (e.g. `?`) or a panic drops the backend before `deinit()` runs.
impl<W: Write> Drop for CrosstermBackend<W> {
    fn drop(&mut self) {
        self.deinit();
    }
}

impl<W: Write> TerminalBackend for CrosstermBackend<W> {
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
        queue!(self.writer, cursor::MoveTo(col, row))
            .map_err(|e| format!("Failed to move cursor: {e}"))?;
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), String> {
        queue!(self.writer, cursor::Hide).map_err(|e| format!("Failed to hide cursor: {e}"))?;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), String> {
        queue!(self.writer, cursor::Show).map_err(|e| format!("Failed to show cursor: {e}"))?;
        Ok(())
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        execute!(self.writer, terminal::Clear(ClearType::UntilNewLine))
            .map_err(|e| format!("Failed to clear to end of line: {e}"))?;
        Ok(())
    }

    fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), String> {
        let style = match shape {
            CursorShape::SteadyBlock => cursor::SetCursorStyle::SteadyBlock,
            CursorShape::SteadyBar => cursor::SetCursorStyle::SteadyBar,
        };
        queue!(self.writer, style).map_err(|e| format!("Failed to set cursor shape: {e}"))?;
        Ok(())
    }
}

impl<W: Write> ColorTerminal for CrosstermBackend<W> {
    fn set_foreground_color(&mut self, color: Color) -> Result<(), String> {
        execute!(self.writer, SetForegroundColor(color_to_crossterm(color)))
            .map_err(|e| format!("Failed to set foreground color: {e}"))?;
        Ok(())
    }

    fn set_background_color(&mut self, color: Color) -> Result<(), String> {
        execute!(self.writer, SetBackgroundColor(color_to_crossterm(color)))
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
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    let alt = modifiers.contains(KeyModifiers::ALT);

    match key_event.code {
        KeyCode::Char(ch) => {
            // Handle Enter key that comes through as character (some terminals send '\r' or '\n')
            if ch == '\r' || ch == '\n' {
                return Key::Enter;
            }
            if ctrl {
                Key::Ctrl(ch as u8)
            } else if alt {
                Key::Alt(ch as u8)
            } else if ch.is_control()
                && ch as u8 >= 1
                && ch as u8 <= 26
                && ch != '\r'
                && ch != '\n'
                && ch != '\t'
                && ch != '\x08'
            {
                // Normalize implicit control characters (e.g. ^W = 23) to Key::Ctrl('w')
                // 'a' is 97, ^A is 1. Offset is 96.
                Key::Ctrl((ch as u8) + 96)
            } else if shift && ch == ' ' {
                Key::ShiftSpace
            } else {
                Key::Char(ch)
            }
        }
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Escape,
        KeyCode::BackTab => Key::ShiftTab,
        KeyCode::Tab => {
            if shift {
                Key::ShiftTab
            } else {
                Key::Tab
            }
        }
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
