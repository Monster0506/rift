//! Movement boundary logic (words, sentences, paragraphs) shared across editor
//! modes. Categorizes chars as whitespace/alphanumeric(+`_`)/symbol, so `foo->bar` is 3 words.

pub mod boundaries;
pub mod buffer;
pub mod classify;

// Re-export commonly used types
pub use classify::{classify_char, is_word_char, CharClass};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
