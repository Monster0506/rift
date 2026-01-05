use crate::buffer::TextBuffer;
use crate::error::RiftError;
use libloading::Library;
use std::sync::Arc;
use streaming_iterator::StreamingIterator;
use tree_sitter::{InputEdit, Parser, Query, QueryCursor, Tree};

pub mod loader;

pub struct Syntax {
    parser: Parser,
    pub tree: Option<Tree>,
    #[allow(dead_code)]
    library: Option<Arc<Library>>,
    #[allow(dead_code)]
    language_name: String,
    pub highlights_query: Option<Query>,
    query_cursor: QueryCursor,
}

impl Syntax {
    pub fn new(
        loaded: loader::LoadedLanguage,
        highlights_source: Option<String>,
    ) -> Result<Self, RiftError> {
        let mut parser = Parser::new();
        parser.set_language(&loaded.language).map_err(|e| {
            RiftError::new(
                crate::error::ErrorType::Internal,
                "LANGUAGE_ERROR",
                e.to_string(),
            )
        })?;

        let highlights_query = if let Some(source) = highlights_source {
            Some(Query::new(&loaded.language, &source).map_err(|e| {
                RiftError::new(
                    crate::error::ErrorType::Internal,
                    "QUERY_ERROR",
                    e.to_string(),
                )
            })?)
        } else {
            None
        };

        Ok(Self {
            parser,
            tree: None,
            library: loaded.library,
            language_name: loaded.name,
            highlights_query,
            query_cursor: QueryCursor::new(),
        })
    }

    pub fn parse(&mut self, text: &TextBuffer) {
        let full_text = text.to_string();
        let tree = self.parser.parse(full_text, self.tree.as_ref());
        self.tree = tree;
    }

    /// Force a full reparse, discarding the old tree
    /// Use this after bulk changes (undo/redo) where incremental parsing would be incorrect
    pub fn reparse(&mut self, text: &TextBuffer) {
        self.tree = None; // Clear old tree to force full parse
        self.parse(text);
    }

    pub fn update(&mut self, edit: InputEdit, new_text: &TextBuffer) {
        if let Some(tree) = self.tree.as_mut() {
            tree.edit(&edit);
        }
        self.parse(new_text);
    }

    pub fn highlights(
        &mut self,
        text: &TextBuffer,
        range: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, String)> {
        let mut result = Vec::new();
        // Destructure to split borrows
        if let Syntax {
            tree: Some(tree),
            highlights_query: Some(query),
            query_cursor,
            ..
        } = self
        {
            let root_node = tree.root_node();

            let full_text = text.to_string();
            let full_bytes = full_text.as_bytes();

            if let Some(r) = range {
                query_cursor.set_byte_range(r);
            } else {
                query_cursor.set_byte_range(0..full_bytes.len());
            }

            let mut matches = query_cursor.matches(query, root_node, full_bytes);

            while let Some(m) = matches.next() {
                for capture in m.captures {
                    let range = capture.node.byte_range();
                    let capture_name = query.capture_names()[capture.index as usize].to_string();
                    result.push((range, capture_name));
                }
            }
        }
        result
    }
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
