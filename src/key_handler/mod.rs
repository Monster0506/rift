//! Key handler
//! Processes keypresses and determines what actions to take
//! Handles special keys that need immediate processing before command translation

use crate::key::Key;
use crate::mode::Mode;

/// Result of processing a keypress
/// Indicates what action the editor should take
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Continue with normal command processing
    Continue,
    /// Skip command processing and re-render (e.g., escape to clear)
    SkipAndRender,
    /// Exit insert mode and re-render
    ExitInsertMode,
    /// Toggle debug mode and re-render
    ToggleDebug,
}

/// Key handler for processing special keypresses
pub struct KeyHandler;

impl KeyHandler {
    /// Process a keypress and determine what action to take
    /// Returns the action the editor should take
    pub fn process_key(
        key: Key,
        current_mode: Mode,
    ) -> KeyAction {
        match current_mode {
            Mode::Normal => Self::process_normal_mode_key(key),
            Mode::Insert => Self::process_insert_mode_key(key),
        }
    }

    /// Process keypress in normal mode
    fn process_normal_mode_key(key: Key) -> KeyAction {
        match key {
            // Debug mode toggle
            Key::Char(b'?') => {
                KeyAction::ToggleDebug
            }
            // Escape - clear pending keys
            Key::Escape => {
                KeyAction::SkipAndRender
            }
            // Ctrl+] - clear pending keys (alternative)
            Key::Ctrl(ch) if ch == b']' => {
                KeyAction::SkipAndRender
            }
            // All other keys continue to command processing
            _ => KeyAction::Continue,
        }
    }

    /// Process keypress in insert mode
    fn process_insert_mode_key(key: Key) -> KeyAction {
        match key {
            // Escape - exit insert mode
            Key::Escape => {
                KeyAction::ExitInsertMode
            }
            // All other keys continue to command processing
            _ => KeyAction::Continue,
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

