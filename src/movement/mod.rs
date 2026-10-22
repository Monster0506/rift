//! Movement boundary logic (words, sentences, paragraphs) shared across editor
//! modes. Categorizes chars as whitespace/alphanumeric(+`_`)/symbol, so `foo->bar` is 3 words.

pub(crate) mod boundaries;
pub(crate) mod buffer;
pub(crate) mod classify;

// Re-export commonly used types

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
