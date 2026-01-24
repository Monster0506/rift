use crate::error::RiftError;
use crate::job_manager::jobs::syntax::SyntaxParseResult;
use crate::syntax::loader::LoadedLanguage;
use std::sync::Arc;
use tree_sitter::{InputEdit, Query, Tree};

pub mod interval_tree;
pub mod loader;
use crate::syntax::interval_tree::IntervalTree;

#[derive(Clone, Debug)]
pub enum SyntaxNotification {
    Loaded { language_name: String },
    HighlightsUpdated,
    Error(String),
}

// Syntax now holds the state directly. The heavyweight parsing happens in jobs.
pub struct Syntax {
    pub language: tree_sitter::Language,
    pub tree: Option<Tree>,
    pub highlights_query: Option<Arc<Query>>,
    pub language_name: String,

    // Cache
    cached_highlights: IntervalTree<u32>,
}

impl Syntax {
    /// Create a new Syntax instance with a loaded language
    pub fn new(
        loaded: LoadedLanguage,
        highlights_query: Option<Arc<Query>>,
    ) -> Result<Self, RiftError> {
        Ok(Self {
            language: loaded.language,
            tree: None,
            highlights_query,
            language_name: loaded.name,
            cached_highlights: IntervalTree::default(),
        })
    }

    /// Update the syntax state from a job result
    pub fn update_from_result(&mut self, result: SyntaxParseResult) {
        if result.language_name != self.language_name {
            // Mismatch? Ignore or warn.
            return;
        }
        self.tree = result.tree;
        self.cached_highlights = result.highlights;
    }

    pub fn update_tree(&mut self, edit: &InputEdit) {
        if let Some(tree) = &mut self.tree {
            tree.edit(edit);
        }
    }

    /// Get current highlights
    pub fn highlights(
        &self,
        range: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, u32)> {
        if let Some(r) = range {
            self.cached_highlights.query(r)
        } else {
            self.cached_highlights
                .iter()
                .map(|(r, v)| (r.clone(), *v))
                .collect()
        }
    }

    /// Get capture names from the query
    pub fn capture_names(&self) -> &[&str] {
        if let Some(query) = &self.highlights_query {
            query.capture_names()
        } else {
            &[]
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
