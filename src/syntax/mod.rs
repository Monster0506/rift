pub(crate) mod interval_tree;
pub(crate) mod loader;

#[cfg(feature = "treesitter")]
mod imp;
#[cfg(feature = "treesitter")]
pub(crate) use imp::{build_syntax, Syntax};
#[cfg(feature = "treesitter")]
pub(crate) use imp::{finalize_highlights, scoped_kept_items, scoped_query_ranges};

#[cfg(not(feature = "treesitter"))]
mod stub;
#[cfg(not(feature = "treesitter"))]
pub(crate) use stub::Syntax;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseOutcome {
    #[cfg(feature = "treesitter")]
    Completed,
    #[cfg(feature = "treesitter")]
    Aborted,
    NoLanguage,
}

pub(crate) const SYNC_PARSE_MAX_BYTES: usize = 256 * 1024;

pub(crate) const SYNC_PARSE_BUDGET: std::time::Duration = std::time::Duration::from_micros(5000);

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
