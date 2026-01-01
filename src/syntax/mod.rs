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
        let tree = self.parser.parse_with(
            &mut |byte, _| {
                if byte >= text.len() {
                    return &[] as &[u8];
                }
                text.get_chunk_at_byte(byte)
            },
            self.tree.as_ref(),
        );
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

            if let Some(r) = range {
                query_cursor.set_byte_range(r);
            } else {
                query_cursor.set_byte_range(0..text.len());
            }

            // Using TextProvider implementation for TextBuffer to avoid full copy
            let mut matches = query_cursor.matches(query, root_node, text);

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

/// Implementation of Tree-sitter's TextProvider to allow efficient querying
/// without copying the entire buffer into a contiguous slice.
impl<'a> tree_sitter::TextProvider<&'a [u8]> for &'a TextBuffer {
    type I = std::vec::IntoIter<&'a [u8]>;

    fn text(&mut self, node: tree_sitter::Node<'_>) -> Self::I {
        let range = node.byte_range();
        // collect into pointers to existing pieces, no data copy
        let chunks: Vec<&'a [u8]> = self.line_index.chunks_in_range(range).collect();
        chunks.into_iter()
    }
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
