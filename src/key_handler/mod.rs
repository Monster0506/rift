//! Key handler
//! Processes keypresses and determines what actions to take
//! Handles special keys that need immediate processing before command translation

/// ## `key_handler`/ Invariants
///
/// - Key handlers translate input events into `Command`s.
/// - Key handlers never mutate buffer or editor state directly.
/// - Key handlers are mode-aware but buffer-agnostic.
/// - Multi-key sequences are handled entirely within this layer.
/// - Invalid or incomplete sequences yield `Noop` or deferred input.
/// - Key handling is deterministic.
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
    /// Exit command mode and re-render
    ExitCommandMode,
    /// Exit search mode and re-render
    ExitSearchMode,
    /// Resize the terminal
    Resize(u16, u16),
}

/// Key handler for processing special keypresses
pub struct KeyHandler;

impl KeyHandler {
    /// Process a keypress and determine what action to take
    /// Returns the action the editor should take
    #[must_use]
    pub fn process_key(key: Key, current_mode: Mode) -> KeyAction {
        if let Key::Resize(cols, rows) = key {
            return KeyAction::Resize(cols, rows);
        }

        match current_mode {
            Mode::Normal => Self::process_normal_mode_key(key),
            Mode::Insert => Self::process_insert_mode_key(key),
            Mode::Command => Self::process_command_mode_key(key),
            Mode::Search => Self::process_search_mode_key(key),
            Mode::Overlay => KeyAction::Continue, // Overlay input handled by editor
        }
    }

    /// Process keypress in normal mode
    fn process_normal_mode_key(key: Key) -> KeyAction {
        match key {
            // Debug mode toggle
            Key::Char('?') => KeyAction::ToggleDebug,
            // Escape - clear pending keys
            Key::Escape => KeyAction::SkipAndRender,
            // Ctrl+] - clear pending keys (alternative)
            Key::Ctrl(b']') => KeyAction::SkipAndRender,
            // All other keys continue to command processing
            _ => KeyAction::Continue,
        }
    }

    /// Process keypress in insert mode
    fn process_insert_mode_key(key: Key) -> KeyAction {
        match key {
            // Escape - exit insert mode
            Key::Escape => KeyAction::ExitInsertMode,
            // All other keys continue to command processing
            _ => KeyAction::Continue,
        }
    }

    /// Process keypress in command mode
    fn process_command_mode_key(key: Key) -> KeyAction {
        match key {
            // Escape - exit command mode back to normal
            Key::Escape => KeyAction::ExitCommandMode,
            // All other keys continue to command processing (for now)
            _ => KeyAction::Continue,
        }
    }

    /// Process keypress in search mode
    fn process_search_mode_key(key: Key) -> KeyAction {
        match key {
            // Escape - exit search mode back to normal
            Key::Escape => KeyAction::ExitSearchMode,
            // All other keys continue to command processing
            _ => KeyAction::Continue,
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
