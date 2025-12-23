//! Rift - A terminal-based text editor

pub mod key;
pub mod mode;
pub mod buffer;
pub mod command;
pub mod executor;
pub mod term;
pub mod viewport;
pub mod render;
pub mod editor;
pub mod state;
pub mod status;
pub mod key_handler;
pub mod floating_window;

#[cfg(test)]
pub mod test_utils;

