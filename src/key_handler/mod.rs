//! Key handler: processes keypresses into actions, handling special keys and multi-key
//! sequences before command translation. Deterministic; never mutates buffer/editor state directly.
use crate::key::InputEvent;
use crate::key::Key;
use crate::key::KeyCode;
use crate::key::Mods;
use crate::mode::Mode;

const CTRL_RIGHT_BRACKET: Key = Key {
    code: KeyCode::Char(']'),
    mods: Mods::CTRL,
};

/// Result of processing a keypress
/// Indicates what action the editor should take
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyAction {
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
struct KeyHandler;

impl KeyHandler {
    /// Process an input event and determine what action to take
    #[must_use]
    const fn process_event(event: &InputEvent, current_mode: Mode) -> KeyAction {
        match event {
            InputEvent::Key(key) => Self::process_key(*key, current_mode),
            InputEvent::Resize(cols, rows) => KeyAction::Resize(*cols, *rows),
            InputEvent::Paste(_) => KeyAction::Continue,
        }
    }

    /// Process a keypress and determine what action to take
    /// Returns the action the editor should take
    #[must_use]
    const fn process_key(key: Key, current_mode: Mode) -> KeyAction {
        match current_mode {
            Mode::Normal => Self::process_normal_mode_key(key),
            Mode::Insert | Mode::Replace => Self::process_insert_mode_key(key),
            Mode::Command | Mode::Rename => Self::process_command_mode_key(key),
            Mode::Search => Self::process_search_mode_key(key),
            Mode::OperatorPending | Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
                KeyAction::Continue
            }
        }
    }

    /// Process keypress in normal mode
    const fn process_normal_mode_key(key: Key) -> KeyAction {
        match key {
            // Debug mode toggle
            Key {
                code: KeyCode::Char('?'),
                mods: Mods::NONE,
            } => KeyAction::ToggleDebug,
            // Escape and Ctrl+] - clear pending keys
            Key {
                code: KeyCode::Esc, ..
            }
            | CTRL_RIGHT_BRACKET => KeyAction::SkipAndRender,
            // All other keys continue to command processing
            _ => KeyAction::Continue,
        }
    }

    /// Process keypress in insert mode
    const fn process_insert_mode_key(key: Key) -> KeyAction {
        match key {
            // Escape - exit insert mode
            Key {
                code: KeyCode::Esc, ..
            } => KeyAction::ExitInsertMode,
            // All other keys continue to command processing
            _ => KeyAction::Continue,
        }
    }

    /// Process keypress in command mode
    const fn process_command_mode_key(key: Key) -> KeyAction {
        match key {
            // Escape - exit command mode back to normal
            Key {
                code: KeyCode::Esc, ..
            } => KeyAction::ExitCommandMode,
            // All other keys continue to command processing (for now)
            _ => KeyAction::Continue,
        }
    }

    /// Process keypress in search mode
    const fn process_search_mode_key(key: Key) -> KeyAction {
        match key {
            // Escape - exit search mode back to normal
            Key {
                code: KeyCode::Esc, ..
            } => KeyAction::ExitSearchMode,
            // All other keys continue to command processing
            _ => KeyAction::Continue,
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
