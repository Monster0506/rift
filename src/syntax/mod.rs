use crate::error::RiftError;
use crate::job_manager::jobs::syntax::SyntaxParseResult;
use crate::syntax::loader::LoadedLanguage;
use std::sync::Arc;
use tree_sitter::{InputEdit, Query, Tree};

pub mod loader;

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
    cached_highlights: Vec<(std::ops::Range<usize>, String)>,
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
            cached_highlights: Vec::new(),
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

    /// Update the tree with an edit (synchronous, fast)
    /// This keeps the tree in sync with buffer edits before the background job runs?
    /// Actually, if we use background jobs, we might not want to touch `tree` here unless we want to keep it valid for incidental queries.
    /// Tree-sitter's `tree.edit()` is fast. We should do it.
    pub fn update_tree(&mut self, edit: &InputEdit) {
        if let Some(tree) = &mut self.tree {
            tree.edit(edit);
        }
    }

    /// Get current highlights
    pub fn highlights(
        &self,
        range: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, String)> {
        if let Some(r) = range {
            self.cached_highlights
                .iter()
                .filter(|(h_range, _)| h_range.start < r.end && h_range.end > r.start)
                .cloned()
                .collect()
        } else {
            self.cached_highlights.clone()
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
