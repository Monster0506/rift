//! Rift - A terminal-based text editor

pub mod action;
pub mod buffer;
pub mod character;
pub mod color;
pub mod command;
pub mod command_line;
pub mod component;
pub mod constants;
pub mod document;
pub mod editor;
pub mod error;
pub mod executor;
pub mod file_explorer;
pub mod floating_window;
pub mod history;
pub mod input_box;
pub mod job_manager;
pub mod key;
pub mod key_handler;
pub mod layer;
pub mod mode;
pub mod movement;
pub mod notification;
pub mod render;
pub mod screen_buffer;
pub mod search;
pub mod select_view;
pub mod state;
pub mod status;
pub mod syntax;
pub mod term;
pub mod undotree_view;
pub mod viewport;

#[cfg(test)]
pub mod test_utils;
