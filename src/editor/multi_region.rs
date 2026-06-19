//! Region-set navigation (`n`/`N` when banked regions exist).

use super::Editor;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// `n`/`N` when the `SelectionSet` is non-empty: cycle the cursor
    /// between banked regions instead of repeat-find/search (design.md S3,
    /// resolved as context-sensitive per this codebase's existing n/N bindings).
    pub(super) fn cycle_to_region(&mut self, forward: bool) -> bool {
        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        if doc.selection_set.is_empty() {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "No regions banked".to_string(),
            );
            return false;
        }
        let cursor = doc.buffer.cursor();
        let target = if forward {
            doc.selection_set.next_region(cursor)
        } else {
            doc.selection_set.prev_region(cursor)
        };
        let Some(region) = target else { return false };
        let (start, _) = region.span();
        let _ = doc.buffer.set_cursor(start);
        true
    }
}
