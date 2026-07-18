//! Terminal backend abstraction
//! Provides platform-agnostic interface for terminal operations

/// ## term/ Invariants
///
/// - Terminal handling is isolated behind a strict abstraction boundary.
/// - Raw mode is enabled before input processing begins.
/// - Terminal state is restored on normal exit and on panic.
/// - Terminal size queries are accurate at the time of use.
/// - Terminal code never depends on editor internals.
use crate::key::Key;

/// Terminal size information
#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    SteadyBlock,
    SteadyBar,
}

/// Terminal backend trait
/// All terminal backends must implement these operations
pub trait TerminalBackend {
    /// Initialize terminal and enter raw mode
    fn init(&mut self) -> Result<(), String>;

    /// Restore terminal to original state
    fn deinit(&mut self);

    /// Check if a key is available within the given duration
    fn poll(&mut self, duration: std::time::Duration) -> Result<bool, String>;

    /// Read and decode a single keypress
    /// Returns None if the event was ignored (e.g. key release)
    fn read_key(&mut self) -> Result<Option<Key>, String>;

    /// Write bytes to stdout
    fn write(&mut self, bytes: &[u8]) -> Result<(), String>;

    /// Flush output buffer
    fn flush(&mut self) -> Result<(), String>;

    /// Get terminal dimensions
    fn get_size(&self) -> Result<Size, String>;

    /// Clear entire screen
    fn clear_screen(&mut self) -> Result<(), String>;

    /// Move cursor to specified position (0-indexed)
    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String>;

    /// Hide cursor
    fn hide_cursor(&mut self) -> Result<(), String>;

    /// Show cursor
    fn show_cursor(&mut self) -> Result<(), String>;

    /// Clear from cursor to end of line
    fn clear_to_end_of_line(&mut self) -> Result<(), String>;

    /// Change the terminal cursor shape (DECSCUSR).
    fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), String>;

    /// Signal that the client should detach without killing the editor.
    /// Only meaningful for IPC backends; default is a no-op.
    fn request_detach(&mut self) {}
}

/// Extension trait for color support
/// Backends that support colors should implement this trait
pub trait ColorTerminal: TerminalBackend {
    /// Set foreground color
    fn set_foreground_color(&mut self, color: crate::color::Color) -> Result<(), String>;

    /// Set background color
    fn set_background_color(&mut self, color: crate::color::Color) -> Result<(), String>;

    /// Reset colors to default
    fn reset_colors(&mut self) -> Result<(), String>;
}

pub mod crossterm;

#[cfg(feature = "terminal_emulation")]
pub mod terminal;
#[cfg(not(feature = "terminal_emulation"))]
#[path = "terminal_stub.rs"]
pub mod terminal;

pub use terminal::Terminal;

/// Events raised by a PTY-backed terminal buffer. Available regardless of the
/// `terminal_emulation` feature so channel plumbing doesn't need its own gate.
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Title(String),
    Wakeup,
    ChildExit(i32),
}
