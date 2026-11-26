use super::Editor;
use crate::document::DocumentId;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// Called once per tick. Notices the active document's buffer revision changing (arms the debounce) and fires an elapsed deadline (spawns `GitGutterDiffJob`).
    pub(super) fn poll_git_gutter_diff(&mut self) {
        let doc_id = self.active_document_id();
        let Some((revision, path)) = self.document_manager.get_document(doc_id).and_then(|doc| {
            if doc.buffer_kind_id() != crate::document::runtime::BufferKindId::FILE {
                return None;
            }
            doc.path()
                .map(|p| (doc.buffer().revision(), p.to_path_buf()))
        }) else {
            return;
        };

        let repo_root = self.git_gutter_repo_root_for(doc_id, &path);
        let Some(repo_root) = repo_root else { return };

        let now = crate::time::Instant::now();
        if self.git_gutter_seen_revision.insert(doc_id, revision) != Some(revision) {
            self.git_gutter_diff.arm(doc_id, now);
        }
        if !self.git_gutter_diff.is_due(doc_id, now) {
            return;
        }

        if let Some(job_id) = self.git_gutter_diff.cancel(doc_id) {
            self.job_manager.cancel_job(job_id);
        }

        let Some(rel_path) = path
            .strip_prefix(&repo_root)
            .ok()
            .map(std::path::Path::to_path_buf)
        else {
            return;
        };
        let buffer_text: String = {
            use crate::buffer::api::BufferView;
            let Some(doc) = self.document_manager.get_document(doc_id) else {
                return;
            };
            doc.buffer()
                .chars(0..doc.buffer().len())
                .map(super::super::character::Character::to_char_lossy)
                .collect()
        };

        let job = crate::job_manager::jobs::git::GitGutterDiffJob::new(
            doc_id,
            revision,
            repo_root,
            rel_path,
            buffer_text,
        );
        let job_id = self.job_manager.spawn(job);
        self.git_gutter_diff.start(doc_id, job_id);
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
        let base = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        // Canonicalize through the same backend `Document::path()` uses (on Windows, `git rev-parse --show-toplevel`'s plain path and `std::fs::canonicalize`'s `\\?\`-prefixed form otherwise fail to `strip_prefix` against each other even when they name the same dir).
        let root = crate::git::discover_repo(base)
            .ok()
            .map(|r| crate::fs_backend::backend().canonicalize(&r.root));
        self.git_gutter_repo_cache.insert(doc_id, root.clone());
        root
    }

    /// Drop cached gutter-diff state for a closed document.
    pub(super) fn clear_git_gutter_state(&mut self, doc_id: DocumentId) {
        self.git_gutter_diff.cancel(doc_id);
        self.git_gutter_seen_revision.remove(&doc_id);
        self.git_gutter_repo_cache.remove(&doc_id);
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
