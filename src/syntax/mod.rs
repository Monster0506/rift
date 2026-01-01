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
        // FIXME: optimize to use chunks instead of collecting all bytes
        // The closure approach was causing trait bound errors with TextProvider
        let bytes = text.line_index.bytes_range(0..text.len());
        let tree = self.parser.parse(&bytes, self.tree.as_ref());
        self.tree = tree;
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
            // FIXME: optimize to avoid full copy
            let bytes = text.line_index.bytes_range(0..text.len());

            if let Some(r) = range {
                query_cursor.set_byte_range(r);
            } else {
                query_cursor.set_byte_range(0..bytes.len());
            }

            // With tree-sitter 0.24, directly passing slice works
            let mut matches = query_cursor.matches(query, root_node, bytes.as_slice());

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
