//! Terminal backend abstraction
//! Provides platform-agnostic interface for terminal operations

/// Terminal handling is isolated behind a strict abstraction boundary with no
/// dependency on editor internals; state is restored on normal exit and panic.
use crate::error::RiftError;
use crate::key::InputEvent;

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
    ///
    /// # Errors
    /// Returns `Err` if the terminal cannot enter raw mode or set up its state.
    fn init(&mut self) -> Result<(), RiftError>;

    /// Restore terminal to original state
    fn deinit(&mut self);

    /// Check if a key is available within the given duration
    ///
    /// # Errors
    /// Returns `Err` if polling for input fails.
    fn poll(&mut self, duration: std::time::Duration) -> Result<bool, RiftError>;

    /// Read and decode a single input event
    /// Returns None if the event was ignored (e.g. key release)
    ///
    /// # Errors
    /// Returns `Err` if reading or decoding the input fails.
    fn read_event(&mut self) -> Result<Option<InputEvent>, RiftError>;

    /// Write bytes to stdout
    ///
    /// # Errors
    /// Returns `Err` if writing to the output fails.
    fn write(&mut self, bytes: &[u8]) -> Result<(), RiftError>;

    /// Flush output buffer
    ///
    /// # Errors
    /// Returns `Err` if flushing the output fails.
    fn flush(&mut self) -> Result<(), RiftError>;

    /// Get terminal dimensions
    ///
    /// # Errors
    /// Returns `Err` if the terminal size cannot be queried.
    fn get_size(&self) -> Result<Size, RiftError>;

    /// Clear entire screen
    ///
    /// # Errors
    /// Returns `Err` if the screen cannot be cleared.
    fn clear_screen(&mut self) -> Result<(), RiftError>;

    /// Move cursor to specified position (0-indexed)
    ///
    /// # Errors
    /// Returns `Err` if the cursor cannot be moved.
    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), RiftError>;

    /// Hide cursor
    ///
    /// # Errors
    /// Returns `Err` if the cursor cannot be hidden.
    fn hide_cursor(&mut self) -> Result<(), RiftError>;

    /// Show cursor
    ///
    /// # Errors
    /// Returns `Err` if the cursor cannot be shown.
    fn show_cursor(&mut self) -> Result<(), RiftError>;

    /// Clear from cursor to end of line
    ///
    /// # Errors
    /// Returns `Err` if the line cannot be cleared.
    fn clear_to_end_of_line(&mut self) -> Result<(), RiftError>;

    /// Change the terminal cursor shape (DECSCUSR).
    ///
    /// # Errors
    /// Returns `Err` if the cursor shape cannot be set.
    fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), RiftError>;

    /// Signal that the client should detach without killing the editor.
    /// Only meaningful for IPC backends; default is a no-op.
    fn request_detach(&mut self) {}
}

pub(crate) mod crossterm;

#[cfg(feature = "terminal_emulation")]
mod terminal;
#[cfg(not(feature = "terminal_emulation"))]
#[path = "terminal_stub.rs"]
mod terminal;

pub(crate) use terminal::Terminal;

/// Events raised by a PTY-backed terminal buffer. Available regardless of the
/// `terminal_emulation` feature so channel plumbing doesn't need its own gate.
#[derive(Debug, Clone)]
pub(crate) enum TerminalEvent {
    Title,
    Wakeup,
    ChildExit(i32),
}
