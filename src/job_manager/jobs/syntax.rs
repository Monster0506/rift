use crate::buffer::TextBuffer;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use crate::syntax::interval_tree::IntervalTree;
use crate::syntax::loader::RawLib;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

#[derive(Debug)]
pub struct SyntaxParseResult {
    pub tree: Option<Tree>,
    pub highlights: IntervalTree<u32>,
    pub language_name: String,
    pub document_id: u64,
    /// Buffer revision at spawn time; the caller must discard this result if
    /// the live revision has since moved on, or a stale parse can clobber newer state.
    pub revision: u64,
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
    revision: u64,
    /// Keeps `parser`'s language's backing dynamic library alive for the job's
    /// lifetime; runs on a background thread detached from the `Syntax` that spawned it.
    _lib: Option<Arc<RawLib>>,
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
        revision: u64,
    ) -> Self {
        Self {
            buffer,
            parser,
            old_tree,
            highlights_query,
            language_name,
            document_id,
            revision,
            _lib: None,
        }
    }

    /// Attach the backing library handle for `parser`'s language, if it was
    /// loaded dynamically, so it stays mapped for the lifetime of this job.
    pub fn with_lib(mut self, lib: Option<Arc<RawLib>>) -> Self {
        self._lib = lib;
        self
    }
}

impl Job for SyntaxParseJob {
    fn name(&self) -> &'static str {
        "syntax-parse"
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        crate::perf_span!(
            "syntax_reparse_job",
            crate::perf::PerfFields {
                bytes: Some(self.buffer.byte_len() as u32),
                ..Default::default()
            }
        );

        // Destructure to avoid partial moves
        let SyntaxParseJob {
            buffer,
            mut parser,
            old_tree,
            highlights_query,
            language_name,
            document_id,
            revision,
            _lib,
        } = *self;

        // Parse using logical bytes so tree-sitter node offsets are in the
        // same coordinate space as the slice we later pass to the query cursor.
        // Previously this used `text.to_string()` (the rendered representation),
        // which expands Control characters to "^X" (2 bytes each) while
        // `to_logical_bytes()` keeps them as a single byte — causing the
        // "range start index out of range" panic when both representations
        // were mixed within the same parse/query cycle.
        let text = buffer;
        let source_bytes = {
            crate::perf_span!(
                "syntax_reparse_job_to_bytes",
                crate::perf::PerfFields {
                    bytes: Some(text.byte_len() as u32),
                    ..Default::default()
                }
            );
            text.to_logical_bytes()
        };

        let tree = {
            crate::perf_span!(
                "syntax_reparse_job_parse",
                crate::perf::PerfFields {
                    bytes: Some(source_bytes.len() as u32),
                    ..Default::default()
                }
            );
            parser.parse(source_bytes.as_slice(), old_tree.as_ref())
        };

        if signal.is_cancelled() {
            return;
        }

        // Highlights
        let mut highlights = Vec::new();
        if let (Some(tree), Some(query)) = (&tree, &highlights_query) {
            crate::perf_span!(
                "syntax_reparse_job_highlights",
                crate::perf::PerfFields {
                    bytes: Some(source_bytes.len() as u32),
                    ..Default::default()
                }
            );
            let root_node = tree.root_node();
            let full_bytes = source_bytes; // same representation as parse

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
            revision,
        };

        let _ = sender.send(JobMessage::Custom(id, Box::new(result)));

        let _ = sender.send(JobMessage::Finished(id, true));
    }

    fn is_silent(&self) -> bool {
        true
    }
}
