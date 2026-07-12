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
