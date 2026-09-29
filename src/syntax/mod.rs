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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseOutcome {
    Completed,
    Aborted,
    NoLanguage,
}

pub(crate) const SYNC_PARSE_MAX_BYTES: usize = 256 * 1024;

pub(crate) const SYNC_PARSE_BUDGET: std::time::Duration = std::time::Duration::from_micros(5000);

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
