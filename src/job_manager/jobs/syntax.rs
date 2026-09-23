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
    /// Logical bytes this job parsed from, cached by the caller so a future
    /// job can patch it instead of rebuilding from scratch.
    pub logical_bytes: Vec<u8>,
}

crate::impl_job_payload!(SyntaxParseResult);

// Manual Debug because Parser and TextBuffer might not impl Debug
pub struct SyntaxParseJob {
    buffer: TextBuffer,
    parser: Parser,
    old_tree: Option<Tree>,
    highlights_query: Option<std::sync::Arc<Query>>,
    language_name: String,
    document_id: u64,
    revision: u64,
    /// Prior logical-bytes buffer and the single edit since it was captured.
    /// Lets `run()` patch instead of rebuilding the logical bytes.
    cached_logical_bytes: Option<Arc<Vec<u8>>>,
    single_edit: Option<InputEdit>,
    /// Keeps `parser`'s language's backing dynamic library alive for the job's
    /// lifetime; runs on a background thread detached from the `Syntax` that spawned it.
    _lib: Option<Arc<RawLib>>,
    /// Highlights as of `old_tree`, plus the single edit since then (if
    /// exactly one landed); otherwise the highlights query rescans everything.
    old_highlights: Arc<IntervalTree<u32>>,
    token: Option<crate::job_manager::AsyncToken>,
}
impl std::fmt::Debug for SyntaxParseJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxParseJob")
            .field("language_name", &self.language_name)
            .field("buffer_len", &self.buffer.len())
            .field("has_old_tree", &self.old_tree.is_some())
            .field("has_query", &self.highlights_query.is_some())
            .field(
                "has_cached_logical_bytes",
                &self.cached_logical_bytes.is_some(),
            )
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
            cached_logical_bytes: None,
            single_edit: None,
            _lib: None,
            old_highlights: Arc::new(IntervalTree::default()),
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
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
        old_highlights: Arc<IntervalTree<u32>>,
        edits: &[InputEdit],
    ) -> Self {
        self.old_highlights = old_highlights;
        self.single_edit = match edits {
            [edit] => Some(*edit),
            _ => None,
        };
        self
    }
    /// Attach a prior logical-bytes buffer and the edit since it was captured,
    /// so `run()` can patch instead of rebuilding; `None`/`None` forces a full rebuild.
    pub fn with_incremental_bytes(
        mut self,
        cached_logical_bytes: Option<Arc<Vec<u8>>>,
        single_edit: Option<tree_sitter::InputEdit>,
    ) -> Self {
        self.cached_logical_bytes = cached_logical_bytes;
        self.single_edit = single_edit;
        self
    }
}

impl Job for SyntaxParseJob {
    fn name(&self) -> &'static str {
        "syntax-parse"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<crate::document::DocumentId> {
        Some(self.document_id)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::SyntaxParse)
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
            cached_logical_bytes,
            single_edit,
            _lib,
            old_highlights,
            token,
        } = *self;

        // Uses logical bytes, not the rendered form, so tree-sitter offsets match
        // the query cursor's coordinate space (control chars differ in byte width).
        let text = buffer;
        let source_bytes = {
            #[cfg_attr(not(feature = "perf_instrumentation"), allow(unused_variables))]
            let will_patch = cached_logical_bytes.is_some() && single_edit.is_some();
            crate::perf_span!(
                "syntax_reparse_job_to_bytes",
                crate::perf::PerfFields {
                    bytes: Some(text.byte_len() as u32),
                    tag: Some(if will_patch { "patch_attempt" } else { "full" }),
                    ..Default::default()
                }
            );
            match (&cached_logical_bytes, &single_edit) {
                (Some(cached), Some(edit)) => text
                    .patch_logical_bytes(
                        cached,
                        edit.start_byte,
                        edit.old_end_byte,
                        edit.new_end_byte,
                    )
                    .unwrap_or_else(|| text.to_logical_bytes()),
                _ => text.to_logical_bytes(),
            }
        };

        let tree = {
            crate::perf_span!(
                "syntax_reparse_job_parse",
                crate::perf::PerfFields {
                    bytes: Some(source_bytes.len() as u32),
                    tag: Some(if old_tree.is_some() {
                        "incremental"
                    } else {
                        "full"
                    }),
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

            // Single-range vec matches the Vec<Range<usize>> the scoped arm returns.
            #[allow(clippy::single_range_in_vec_init)]
            let query_ranges: Vec<std::ops::Range<usize>> = match scoped {
                Some((prev_tree, edit)) => {
                    crate::syntax::scoped_query_ranges(prev_tree, tree, edit)
                }
                None => vec![0..source_bytes.len()],
            };

            let mut fresh: Vec<(std::ops::Range<usize>, u32, usize)> = Vec::new();
            for range in &query_ranges {
                let mut cursor = QueryCursor::new();
                cursor.set_byte_range(range.clone());
                let mut matches = cursor.matches(query, root_node, source_bytes.as_slice());
                while let Some(m) = matches.next() {
                    if signal.is_cancelled() {
                        return;
                    }
                    let pattern_index = m.pattern_index;
                    for capture in m.captures() {
                        fresh.push((capture.node.byte_range(), capture.index, pattern_index));
                    }
                }
            }

            if let Some((_, edit)) = scoped {
                let kept = crate::syntax::scoped_kept_items(&old_highlights, edit, &query_ranges);
                highlights.extend(kept.into_iter().map(|(r, c)| (r, c, 0)));
            }
            highlights.extend(fresh);
        }

        let result = SyntaxParseResult {
            tree,
            highlights: IntervalTree::new(crate::syntax::finalize_highlights(highlights)),
            language_name,
            document_id,
            revision,
            logical_bytes: source_bytes,
        };

        if let Some(token) = token {
            crate::job_manager::send_job_result_with_token(&sender, id, token, Box::new(result));
        } else {
            crate::job_manager::send_job_result(&sender, id, Box::new(result));
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[cfg(feature = "treesitter")]
#[path = "syntax_tests.rs"]
mod syntax_tests;
