use crate::buffer::TextBuffer;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use crate::syntax::interval_tree::IntervalTree;
use std::sync::mpsc::Sender;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

#[derive(Debug)]
pub struct SyntaxParseResult {
    pub tree: Option<Tree>,
    pub highlights: IntervalTree<u32>,
    pub language_name: String,
    pub document_id: u64,
}

impl crate::job_manager::JobPayload for SyntaxParseResult {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

// Manual Debug because Parser and TextBuffer might not impl Debug
pub struct SyntaxParseJob {
    buffer: TextBuffer,
    parser: Parser,
    old_tree: Option<Tree>,
    highlights_query: Option<std::sync::Arc<Query>>,
    language_name: String,
    document_id: u64,
}

impl std::fmt::Debug for SyntaxParseJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxParseJob")
            .field("language_name", &self.language_name)
            .field("buffer_len", &self.buffer.len())
            .field("has_old_tree", &self.old_tree.is_some())
            .field("has_query", &self.highlights_query.is_some())
            .finish()
    }
}

impl SyntaxParseJob {
    pub fn new(
        buffer: TextBuffer,
        parser: Parser,
        old_tree: Option<Tree>,
        highlights_query: Option<std::sync::Arc<Query>>,
        language_name: String,
        document_id: u64,
    ) -> Self {
        Self {
            buffer,
            parser,
            old_tree,
            highlights_query,
            language_name,
            document_id,
        }
    }
}

impl Job for SyntaxParseJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        // Destructure to avoid partial moves
        let SyntaxParseJob {
            buffer,
            mut parser,
            old_tree,
            highlights_query,
            language_name,
            document_id,
        } = *self;

        // Parse
        let text = buffer;
        let source_code = text.to_string();
        let tree = parser.parse(&source_code, old_tree.as_ref());

        if signal.is_cancelled() {
            return;
        }

        // Highlights
        let mut highlights = Vec::new();
        if let (Some(tree), Some(query)) = (&tree, &highlights_query) {
            let root_node = tree.root_node();
            let full_bytes = text.to_logical_bytes();

            let mut cursor = QueryCursor::new();
            cursor.set_byte_range(0..full_bytes.len());

            let mut matches = cursor.matches(query, root_node, full_bytes.as_slice());

            while let Some(m) = matches.next() {
                if signal.is_cancelled() {
                    return;
                }
                for capture in m.captures {
                    let range = capture.node.byte_range();
                    // Store index instead of allocating string
                    let capture_index = capture.index;
                    highlights.push((range, capture_index));
                }
            }
        }

        let result = SyntaxParseResult {
            tree,
            highlights: IntervalTree::new(highlights),
            language_name,
            document_id,
        };

        let _ = sender.send(JobMessage::Custom(id, Box::new(result)));

        let _ = sender.send(JobMessage::Finished(id, true));
    }

    fn is_silent(&self) -> bool {
        true
    }
}
