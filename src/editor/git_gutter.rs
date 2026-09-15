//! Debounced git-gutter-diff (Phase 5): re-diffs the active `File` buffer's live content against its git index after edits settle, coloring the line-number gutter per changed line. Revision-driven rather than hooked into every edit call site; `poll_git_gutter_diff` runs once per tick (mirroring.

use super::Editor;
use crate::document::DocumentId;
use crate::term::TerminalBackend;
use std::time::Duration;

/// Debounce window after an edit before re-diffing;  coalesces a burst of
/// keystrokes into one job, same rationale as `SYNTAX_REPARSE_DEBOUNCE`.
const GIT_GUTTER_DEBOUNCE: Duration = Duration::from_millis(400);

#[derive(Default)]
pub(super) struct PendingGitGutterDiff {
    /// Buffer revision this state was last armed/diffed for. `None` means "never observed this document"; a file loaded via the bulk `from_bytes` path (skipping `insert_str`) starts at revision 0, the same value `u64::default()` would give a brand-new tracker entry, so a bare `u64` here would mistake "never seen".
    last_seen_revision: Option<u64>,
    debounce_deadline: Option<crate::time::Instant>,
    in_flight_job: Option<usize>,
}

impl<T: TerminalBackend> Editor<T> {
    /// Called once per tick. Notices the active document's buffer revision changing (arms the debounce) and fires an elapsed deadline (spawns `GitGutterDiffJob`).
    pub(super) fn poll_git_gutter_diff(&mut self) {
        let doc_id = self.active_document_id();
        let Some((revision, path)) = self.document_manager.get_document(doc_id).and_then(|doc| {
            if !matches!(doc.kind, crate::document::BufferKind::File) {
                return None;
            }
            doc.path().map(|p| (doc.buffer.revision, p.to_path_buf()))
        }) else {
            return;
        };

        let repo_root = self.git_gutter_repo_root_for(doc_id, &path);
        let Some(repo_root) = repo_root else { return };

        let now = crate::time::Instant::now();
        let due = {
            let entry = self.pending_git_gutter_diff.entry(doc_id).or_default();
            if entry.last_seen_revision != Some(revision) {
                entry.last_seen_revision = Some(revision);
                entry.debounce_deadline = Some(now + GIT_GUTTER_DEBOUNCE);
            }
            entry.debounce_deadline.is_some_and(|d| now >= d)
        };
        if !due {
            return;
        }

        let stale_job = {
            let entry = self.pending_git_gutter_diff.entry(doc_id).or_default();
            entry.debounce_deadline = None;
            entry.in_flight_job.take()
        };
        if let Some(job_id) = stale_job {
            self.job_manager.cancel_job(job_id);
        }

        let Some(rel_path) = path.strip_prefix(&repo_root).ok().map(|p| p.to_path_buf()) else {
            return;
        };
        let buffer_text: String = {
            use crate::buffer::api::BufferView;
            let Some(doc) = self.document_manager.get_document(doc_id) else {
                return;
            };
            doc.buffer
                .chars(0..doc.buffer.len())
                .map(|c| c.to_char_lossy())
                .collect()
        };

        let job = crate::job_manager::jobs::git::GitGutterDiffJob::new(
            doc_id as usize,
            revision,
            repo_root,
            rel_path,
            buffer_text,
        );
        let job_id = self.job_manager.spawn(job);
        self.pending_git_gutter_diff
            .entry(doc_id)
            .or_default()
            .in_flight_job = Some(job_id);
    }

    /// Memoized repo-root lookup for gutter-diff polling;  `git rev-parse`
    /// once per document, not once per tick.
    fn git_gutter_repo_root_for(
        &mut self,
        doc_id: DocumentId,
        path: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        if let Some(cached) = self.git_gutter_repo_cache.get(&doc_id) {
            return cached.clone();
        }
        let base = path.parent().unwrap_or(std::path::Path::new("."));
        // Canonicalize through the same backend `Document::path()` uses (on Windows, `git rev-parse --show-toplevel`'s plain path and `std::fs::canonicalize`'s `\\?\`-prefixed form otherwise fail to `strip_prefix` against each other even when they name the same dir).
        let root = crate::git::discover_repo(base)
            .ok()
            .map(|r| crate::fs_backend::backend().canonicalize(&r.root));
        self.git_gutter_repo_cache.insert(doc_id, root.clone());
        root
    }
}

/// Live gutter-diff signs for `doc`, mapped to line-number foreground colors for `DrawContext::git_gutter_colors`. Empty for anything but a `File` buffer with signs currently recorded.
pub(super) fn git_gutter_render_colors(
    doc: &crate::document::Document,
) -> Vec<(usize, crate::color::Color)> {
    use crate::color::Color;
    use crate::git::diff::GutterSignKind;

    doc.annotations
        .git_gutter_signs()
        .into_iter()
        .map(|(line, kind)| {
            let color = match kind {
                GutterSignKind::Add => Color::Green,
                GutterSignKind::Change => Color::Yellow,
                GutterSignKind::Delete => Color::Red,
            };
            (line, color)
        })
        .collect()
}
