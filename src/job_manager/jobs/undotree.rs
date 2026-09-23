use crate::color::Color;
use crate::document::DocumentId;
use crate::history::{EditSeq, UndoTree};
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::ops::Range;
use std::sync::mpsc::Sender;

/// Result of a background undo-tree render
#[derive(Debug)]
pub struct UndoTreeRenderResult {
    /// The undotree buffer document to update
    pub ut_doc_id: DocumentId,
    /// Rendered text content
    pub text: String,
    /// Per-line sequence mapping (u64::MAX = connector line)
    pub sequences: Vec<EditSeq>,
    /// Per-byte-range foreground colour highlights
    pub highlights: Vec<(Range<usize>, Color)>,
}

crate::impl_job_payload!(UndoTreeRenderResult);

/// Job that renders an undo-tree to text in a background thread, on a cloned
/// snapshot so the expensive `render_tree_to_text` call never blocks the main thread.
pub struct UndoTreeRenderJob {
    ut_doc_id: DocumentId,
    tree: UndoTree,
    token: Option<crate::job_manager::AsyncToken>,
}

impl std::fmt::Debug for UndoTreeRenderJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UndoTreeRenderJob")
            .field("ut_doc_id", &self.ut_doc_id)
            .finish_non_exhaustive()
    }
}

impl UndoTreeRenderJob {
    pub fn new(ut_doc_id: DocumentId, tree: UndoTree) -> Self {
        Self {
            ut_doc_id,
            tree,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }
}

impl Job for UndoTreeRenderJob {
    fn name(&self) -> &'static str {
        "undotree-render"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<DocumentId> {
        Some(self.ut_doc_id)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::UndoTree)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        let (text, sequences, highlights) = crate::undotree_view::render_tree_to_text(&self.tree);

        if signal.is_cancelled() {
            return;
        }

        let result = Box::new(UndoTreeRenderResult {
            ut_doc_id: self.ut_doc_id,
            text,
            sequences,
            highlights,
        });
        if let Some(token) = self.token {
            crate::job_manager::send_job_result_with_token(&sender, id, token, result);
        } else {
            crate::job_manager::send_job_result(&sender, id, result);
        }
    }
    fn is_silent(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "undotree_tests.rs"]
mod undotree_tests;
