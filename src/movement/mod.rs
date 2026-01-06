//! Movement semantics for all navigation operations
//!
//! This module centralizes the logic for determining movement boundaries
//! (words, sentences, paragraphs) across all editor modes.
//!
//! ## Design
//!
//! Movement is based on character categories:
//! - **Whitespace**: spaces, tabs, newlines
//! - **Alphanumeric**: letters, numbers, and underscore
//! - **Symbol**: all other characters (punctuation, operators, etc.)
//!
//! This means:
//! - `hello_world` is ONE word (underscore is alphanumeric)
//! - `foo->bar` is THREE words: `foo`, `->`, `bar`
//! - The same semantics apply in insert mode, command mode, and search mode
//!
//! ## Modules
//!
//! - [`classify`] - Character classification functions
//! - [`boundaries`] - String-based word boundary detection (for command line)
//! - [`buffer`] - Buffer-based word movement (for insert mode)

pub mod boundaries;
pub mod buffer;
pub mod classify;

// Re-export commonly used types
pub use classify::{classify_char, is_word_char, CharClass};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
