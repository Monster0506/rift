use crate::buffer::TextBuffer;
use crate::document::DocumentId;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use tree_sitter::{Parser, Tree};

#[derive(Debug)]
pub(crate) struct SyntaxParseResult {
    pub(crate) tree: Option<Tree>,
    pub(crate) language_name: String,
    pub(crate) document_id: DocumentId,
    /// Buffer revision at spawn time; the caller must discard this result if
    /// the live revision has since moved on, or a stale parse can clobber newer state.
    pub(crate) revision: u64,
}

crate::job_manager::impl_job_payload!(SyntaxParseResult);

// Manual Debug because Parser and TextBuffer might not impl Debug
pub(crate) struct SyntaxParseJob {
    buffer: TextBuffer,
    parser: Parser,
    old_tree: Option<Tree>,
    language_name: String,
    document_id: DocumentId,
    revision: u64,
    /// Keeps `parser`'s language's backing dynamic library alive for the job's
    /// lifetime; runs on a background thread detached from the `Syntax` that spawned it.
    lib_handle: Option<Arc<libloading::Library>>,
    token: Option<crate::job_manager::AsyncToken>,
}
impl std::fmt::Debug for SyntaxParseJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxParseJob")
            .field("language_name", &self.language_name)
            .field("buffer_len", &self.buffer.len())
            .field("has_old_tree", &self.old_tree.is_some())
            .finish_non_exhaustive()
    }
}

impl SyntaxParseJob {
    pub(crate) fn new(
        buffer: TextBuffer,
        parser: Parser,
        old_tree: Option<Tree>,
        language_name: String,
        document_id: DocumentId,
        revision: u64,
    ) -> Self {
        Self {
            buffer,
            parser,
            old_tree,
            language_name,
            document_id,
            revision,
            lib_handle: None,
            token: None,
        }
    }

    #[must_use]
    pub(crate) const fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }

    /// Attach the backing library handle for `parser`'s language, if it was
    /// loaded dynamically, so it stays mapped for the lifetime of this job.
    #[must_use]
    pub(crate) fn with_lib(mut self, lib: Option<Arc<libloading::Library>>) -> Self {
        self.lib_handle = lib;
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
                bytes: Some(u32::try_from(self.buffer.byte_len()).unwrap_or(u32::MAX)),
                ..Default::default()
            }
        );

        // Destructure to avoid partial moves
        let Self {
            buffer,
            mut parser,
            old_tree,
            language_name,
            document_id,
            revision,
            lib_handle: _lib,
            token,
        } = *self;

        let tree = crate::syntax::parse_buffer(&mut parser, &buffer, old_tree.as_ref());

        if signal.is_cancelled() {
            return;
        }

        let result = SyntaxParseResult {
            tree,
            language_name,
            document_id,
            revision,
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
