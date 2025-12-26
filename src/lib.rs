//! Rift - A terminal-based text editor

pub mod buffer;
pub mod color;
pub mod command;
pub mod command_line;
pub mod document;
pub mod editor;
pub mod error;
pub mod executor;
pub mod floating_window;
pub mod key;
pub mod key_handler;
pub mod layer;
pub mod mode;
pub mod notification;
pub mod render;
pub mod screen_buffer;
pub mod state;
pub mod status;
pub mod term;
pub mod viewport;

#[cfg(test)]
pub mod test_utils;
