use crate::color::Color;
use crate::document::DocumentId;
use crate::history::{EditSeq, UndoTree};
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload};
use std::any::Any;
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

impl JobPayload for UndoTreeRenderResult {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// Job that renders an undo-tree to text in a background thread.
///
/// Takes a snapshot (clone) of the `UndoTree` — the potentially expensive
/// `render_tree_to_text` call therefore never blocks the main thread.
pub struct UndoTreeRenderJob {
    ut_doc_id: DocumentId,
    tree: UndoTree,
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
        Self { ut_doc_id, tree }
    }
}

impl Job for UndoTreeRenderJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        let (text, sequences, highlights) =
            crate::undotree_view::render_tree_to_text(&self.tree);

        if signal.is_cancelled() {
            return;
        }

        let result = Box::new(UndoTreeRenderResult {
            ut_doc_id: self.ut_doc_id,
            text,
            sequences,
            highlights,
        });

        let _ = sender.send(JobMessage::Custom(id, result));
        let _ = sender.send(JobMessage::Finished(id, true));
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::UndoTree;

    fn make_test_tree() -> UndoTree {
        use crate::history::{EditNode, EditTransaction};
        let mut tree = UndoTree::new();
        tree.nodes.clear();
        tree.root_seq = 0;
        tree.nodes.insert(0, EditNode::new(0, EditTransaction::new("root"), None));
        tree.nodes.insert(1, EditNode::new(1, EditTransaction::new("edit1"), Some(0)));
        tree.nodes.get_mut(&0).unwrap().children.push(1);
        tree.current = 1;
        tree
    }

    #[test]
    fn test_undotree_render_result_implements_job_payload() {
        let result = UndoTreeRenderResult {
            ut_doc_id: 1,
            text: "* [1] edit1\n* [0] root".to_string(),
            sequences: vec![1, 0],
            highlights: vec![(0..1, Color::Magenta)],
        };
        // JobPayload::as_any downcasting round-trip
        let boxed: Box<dyn JobPayload> = Box::new(result);
        assert!(boxed.as_any().downcast_ref::<UndoTreeRenderResult>().is_some());
    }

    #[test]
    fn test_undotree_render_job_produces_result() {
        use crate::job_manager::{CancellationSignal, JobMessage};
        use std::sync::{mpsc, Arc};
        use std::sync::atomic::AtomicBool;

        let tree = make_test_tree();
        let job = Box::new(UndoTreeRenderJob::new(42, tree));
        let (tx, rx) = mpsc::channel();
        let signal = CancellationSignal {
            cancelled: Arc::new(AtomicBool::new(false)),
        };

        job.run(1, tx, signal);

        let messages: Vec<JobMessage> = rx.try_iter().collect();
        // Should have a Custom payload and a Finished message
        let has_custom = messages.iter().any(|m| matches!(m, JobMessage::Custom(1, _)));
        let has_finished = messages.iter().any(|m| matches!(m, JobMessage::Finished(1, true)));
        assert!(has_custom, "expected Custom message");
        assert!(has_finished, "expected Finished message");
    }

    #[test]
    fn test_undotree_render_job_result_content() {
        use crate::job_manager::{CancellationSignal, JobMessage};
        use std::sync::{mpsc, Arc};
        use std::sync::atomic::AtomicBool;

        let tree = make_test_tree();
        let job = Box::new(UndoTreeRenderJob::new(7, tree));
        let (tx, rx) = mpsc::channel();
        let signal = CancellationSignal {
            cancelled: Arc::new(AtomicBool::new(false)),
        };

        job.run(1, tx, signal);

        for msg in rx.try_iter() {
            if let JobMessage::Custom(_, payload) = msg {
                let result = payload
                    .into_any()
                    .downcast::<UndoTreeRenderResult>()
                    .expect("payload should be UndoTreeRenderResult");
                assert_eq!(result.ut_doc_id, 7);
                assert!(!result.text.is_empty());
                assert!(!result.sequences.is_empty());
                // sequences count must match line count
                assert_eq!(result.sequences.len(), result.text.lines().count());
                return;
            }
        }
        panic!("no Custom message received");
    }

    #[test]
    fn test_undotree_render_job_cancelled_before_run() {
        use crate::job_manager::{CancellationSignal, JobMessage};
        use std::sync::{mpsc, Arc};
        use std::sync::atomic::AtomicBool;

        let tree = make_test_tree();
        let job = Box::new(UndoTreeRenderJob::new(1, tree));
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(true)); // pre-cancelled
        let signal = CancellationSignal { cancelled };

        job.run(1, tx, signal);

        let messages: Vec<JobMessage> = rx.try_iter().collect();
        // Should produce no Custom message when cancelled
        assert!(!messages.iter().any(|m| matches!(m, JobMessage::Custom(_, _))));
    }

    #[test]
    fn test_undotree_render_job_is_silent() {
        let tree = make_test_tree();
        let job = UndoTreeRenderJob::new(1, tree);
        assert!(job.is_silent());
    }
}
