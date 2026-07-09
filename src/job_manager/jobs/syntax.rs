use crate::buffer::TextBuffer;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use crate::syntax::interval_tree::IntervalTree;
use crate::syntax::loader::RawLib;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use streaming_iterator::StreamingIterator;
use tree_sitter::{InputEdit, Parser, Query, QueryCursor, Tree};

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
    /// Highlights as of `old_tree`, plus the single edit since then (if
    /// exactly one landed); otherwise the highlights query rescans everything.
    old_highlights: IntervalTree<u32>,
    single_edit: Option<InputEdit>,
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
            old_highlights: IntervalTree::default(),
            single_edit: None,
        }
    }

    /// Attach the backing library handle for `parser`'s language, if it was
    /// loaded dynamically, so it stays mapped for the lifetime of this job.
    pub fn with_lib(mut self, lib: Option<Arc<RawLib>>) -> Self {
        self._lib = lib;
        self
    }

    /// Attach the previous highlights and edits since the last completed parse,
    /// so highlights can be scoped to the changed region (only 1 edit is optimized).
    pub fn with_highlights_context(
        mut self,
        old_highlights: IntervalTree<u32>,
        edits: &[InputEdit],
    ) -> Self {
        self.old_highlights = old_highlights;
        self.single_edit = match edits {
            [edit] => Some(*edit),
            _ => None,
        };
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
            old_highlights,
            single_edit,
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
                    tag: Some(if old_tree.is_some() { "incremental" } else { "full" }),
                    ..Default::default()
                }
            );
            parser.parse(source_bytes.as_slice(), old_tree.as_ref())
        };

        if signal.is_cancelled() {
            return;
        }

        // On a single-edit incremental reparse, scope the query to what
        // tree-sitter says changed and reuse the rest of the old highlights.
        let mut highlights = Vec::new();
        if let (Some(tree), Some(query)) = (&tree, &highlights_query) {
            let scoped = match (old_tree.as_ref(), single_edit) {
                (Some(prev_tree), Some(edit)) => Some((prev_tree, edit)),
                _ => None,
            };
            crate::perf_span!(
                "syntax_reparse_job_highlights",
                crate::perf::PerfFields {
                    bytes: Some(source_bytes.len() as u32),
                    tag: Some(if scoped.is_some() { "scoped" } else { "full" }),
                    ..Default::default()
                }
            );
            let root_node = tree.root_node();
            let full_bytes = source_bytes; // same representation as parse

            let query_ranges: Vec<std::ops::Range<usize>> = match scoped {
                Some((prev_tree, edit)) => {
                    let (ranges, kept) =
                        crate::syntax::scoped_requery_plan(prev_tree, tree, edit, &old_highlights);
                    highlights = kept;
                    ranges
                }
                None => vec![0..full_bytes.len()],
            };

            for range in query_ranges {
                let mut cursor = QueryCursor::new();
                cursor.set_byte_range(range);
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

            if scoped.is_some() {
                // set_byte_range matches on a pattern's whole span, so a
                // scoped requery can re-emit a capture already kept below.
                highlights.sort_by(|a, b| {
                    a.0.start
                        .cmp(&b.0.start)
                        .then(a.0.end.cmp(&b.0.end))
                        .then(a.1.cmp(&b.1))
                });
                highlights.dedup();
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

#[cfg(test)]
#[cfg(feature = "treesitter")]
mod tests {
    use super::*;
    use crate::syntax::loader::LanguageLoader;
    use std::path::PathBuf;
    use std::sync::{atomic::AtomicBool, mpsc};

    fn make_signal() -> CancellationSignal {
        CancellationSignal {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn rust_language_and_query() -> (tree_sitter::Language, Arc<Query>) {
        let loader = LanguageLoader::new(PathBuf::from("."));
        let loaded = loader.load_language("rust").expect("rust grammar");
        let query_src = loader
            .load_query("rust", "highlights")
            .expect("rust highlights query");
        let query = Query::new(&loaded.language, &query_src).expect("compile highlights query");
        (loaded.language, Arc::new(query))
    }

    fn make_buffer(src: &str) -> TextBuffer {
        let mut buffer = TextBuffer::new(src.len().max(16)).unwrap();
        buffer.insert_str(src).unwrap();
        buffer
    }

    fn run_job(job: SyntaxParseJob) -> SyntaxParseResult {
        let (tx, rx) = mpsc::channel();
        Box::new(job).run(1, tx, make_signal());

        let mut result = None;
        for msg in rx {
            if let JobMessage::Custom(_, payload) = msg {
                result = payload
                    .into_any()
                    .downcast::<SyntaxParseResult>()
                    .ok()
                    .map(|b| *b);
            }
        }
        result.expect("SyntaxParseJob did not produce a SyntaxParseResult")
    }

    fn sorted_highlights(tree: &IntervalTree<u32>) -> Vec<(std::ops::Range<usize>, u32)> {
        let mut items: Vec<_> = tree.iter().map(|(r, v)| (r.clone(), *v)).collect();
        items.sort_by(|a, b| {
            a.0.start
                .cmp(&b.0.start)
                .then(a.0.end.cmp(&b.0.end))
                .then(a.1.cmp(&b.1))
        });
        items
    }

    /// A scoped single-edit reparse must match a full from-scratch parse of
    /// the same edited content.
    #[test]
    fn test_scoped_highlights_match_full_recompute_after_single_edit() {
        let (language, query) = rust_language_and_query();

        let initial_src = "fn foo() { let a = 1; let b = 2; }";
        let buffer1 = make_buffer(initial_src);

        let mut parser1 = Parser::new();
        parser1.set_language(&language).unwrap();
        let tree1 = parser1
            .parse(buffer1.to_logical_bytes().as_slice(), None)
            .unwrap();

        let initial_highlights = {
            let root = tree1.root_node();
            let mut cursor = QueryCursor::new();
            cursor.set_byte_range(0..buffer1.byte_len());
            let source = buffer1.to_logical_bytes();
            let mut matches = cursor.matches(&query, root, source.as_slice());
            let mut items = Vec::new();
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    items.push((capture.node.byte_range(), capture.index));
                }
            }
            IntervalTree::new(items)
        };

        // Insert one character ("1" -> "11") inside the first literal.
        let insert_pos = initial_src.find("1;").unwrap() + 1;
        let mut buffer2 = buffer1.clone();
        buffer2.set_cursor(insert_pos).unwrap();
        buffer2.insert_str("1").unwrap();

        let edit = InputEdit {
            start_byte: insert_pos,
            old_end_byte: insert_pos,
            new_end_byte: insert_pos + 1,
            start_position: tree_sitter::Point {
                row: 0,
                column: insert_pos,
            },
            old_end_position: tree_sitter::Point {
                row: 0,
                column: insert_pos,
            },
            new_end_position: tree_sitter::Point {
                row: 0,
                column: insert_pos + 1,
            },
        };
        let mut old_tree_edited = tree1.clone();
        old_tree_edited.edit(&edit);

        let mut parser2 = Parser::new();
        parser2.set_language(&language).unwrap();
        let job = SyntaxParseJob::new(
            buffer2.clone(),
            parser2,
            Some(old_tree_edited),
            Some(query.clone()),
            "rust".to_string(),
            1,
            buffer2.revision,
        )
        .with_highlights_context(initial_highlights.clone(), std::slice::from_ref(&edit));

        let result = run_job(job);

        // Ground truth: a full from-scratch parse and query of the edited content.
        let mut parser3 = Parser::new();
        parser3.set_language(&language).unwrap();
        let expected_source = buffer2.to_logical_bytes();
        let expected_tree = parser3.parse(expected_source.as_slice(), None).unwrap();
        let expected_highlights = {
            let root = expected_tree.root_node();
            let mut cursor = QueryCursor::new();
            cursor.set_byte_range(0..expected_source.len());
            let mut matches = cursor.matches(&query, root, expected_source.as_slice());
            let mut items = Vec::new();
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    items.push((capture.node.byte_range(), capture.index));
                }
            }
            IntervalTree::new(items)
        };

        assert_eq!(
            sorted_highlights(&result.highlights),
            sorted_highlights(&expected_highlights),
        );
    }
}
