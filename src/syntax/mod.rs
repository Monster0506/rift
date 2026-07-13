pub mod interval_tree;
pub mod loader;

#[cfg(feature = "treesitter")]
mod imp;
#[cfg(feature = "treesitter")]
pub use imp::{build_syntax, InjectedLayer, Syntax};
#[cfg(feature = "treesitter")]
pub(crate) use imp::{finalize_highlights, scoped_kept_items, scoped_query_ranges};

#[cfg(not(feature = "treesitter"))]
mod stub;
#[cfg(not(feature = "treesitter"))]
pub use stub::Syntax;

#[derive(Clone, Debug)]
pub enum SyntaxNotification {
    Loaded { language_name: String },
    HighlightsUpdated,
    Error(String),
}

/// Result of a budgeted synchronous parse attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseOutcome {
    /// Parsed and re-highlighted within the budget; state is up to date.
    Completed,
    /// Exceeded the time budget; state is unchanged, caller should background it.
    Aborted,
    /// No language configured for this document.
    NoLanguage,
}

#[cfg(test)]
mod tests {
    use crate::buffer::TextBuffer;

    #[test]
    fn test_text_provider_chunks() {
        let mut buffer = TextBuffer::new(100).unwrap();
        buffer.insert_str("line1\nline2\nline3").unwrap();

        assert_eq!(buffer.to_string(), "line1\nline2\nline3");
    }

    #[test]
    fn test_syntax_new_placeholder() {
        // Basic test to ensure TextBuffer is usable
        let buffer = TextBuffer::new(10).unwrap();
        assert_eq!(buffer.len(), 0);
    }
}
