//! Git blame view: open, populate, and walk-back navigation. Read-only; no `:w` reconciliation.

use super::Editor;
use crate::document::BufferKind;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// Open a blame view for `path`'s current content, as a new vertical split next to it. `repo_root` and `path` are caller-supplied; callers resolve "which file" from their own context (the active file buffer, a Status entry under the cursor, a `:Git blame <path>` argument), never guessed here. If `path` isn't.
    pub fn open_git_blame(&mut self, path: std::path::PathBuf, repo_root: std::path::PathBuf) {
        let linked_doc_id = self
            .document_manager
            .documents_iter()
            .find(|d| d.path() == Some(path.as_path()))
            .map(|d| d.id);
        let linked_doc_id = match linked_doc_id {
            Some(id) => id,
            None => {
                if let Err(e) = self.open_file(Some(path.display().to_string()), false) {
                    self.state.handle_error(e);
                    return;
                }
                self.active_document_id()
            }
        };

        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let rows = size.rows as usize;
        let cols = size.cols as usize;

        let blame_doc_id = self.document_manager.next_id();
        let blame_doc = match crate::document::Document::new_git_blame(
            blame_doc_id,
            repo_root.clone(),
            linked_doc_id,
            path.clone(),
            None,
        ) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        self.document_manager.add_document(blame_doc);

        let focused = self.split_tree.focused_window_id();
        let blame_win_id = self
            .split_tree
            .split(
                crate::split::tree::SplitDirection::Vertical,
                focused,
                blame_doc_id,
                rows,
                cols,
            )
            .expect("focused window is always a valid leaf");
        self.split_tree.set_focus(blame_win_id);
        let _ = self.document_manager.switch_to_document(blame_doc_id);

        let job = crate::job_manager::jobs::git::GitBlameJob::new(
            blame_doc_id as usize,
            repo_root,
            path,
            None,
        );
        self.job_manager.spawn(job);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// `Enter` on a blame line: walk back one commit; re-blame the whole file as of that commit's parent (`<sha>^`), so the line's history before the shown commit becomes visible.
    pub(super) fn git_blame_walk_back(&mut self) {
        let (repo_root, path, sha) = {
            let doc = self.active_document();
            let (repo_root, path) = match &doc.kind {
                BufferKind::GitBlame {
                    repo_root, path, ..
                } => (repo_root.clone(), path.clone()),
                _ => return,
            };
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);
            let Some(sha) = doc.annotations.git_blame_sha_at_line(line) else {
                return;
            };
            (repo_root, path, sha)
        };

        // A root commit has no parent;  walking back from it would just
        // fail the `GitBlameJob` with a raw "bad revision" git error.
        let has_parent = crate::git::run(
            &repo_root,
            &["rev-parse", "--verify", "--quiet", &format!("{sha}^")],
        )
        .map(|out| out.success)
        .unwrap_or(false);
        if !has_parent {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "This commit introduced the line — nothing earlier to walk back to".to_string(),
            );
            return;
        }

        let doc_id = self.active_document_id();
        let parent_commit = format!("{sha}^");
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            if let BufferKind::GitBlame { at_commit, .. } = &mut doc.kind {
                *at_commit = Some(parent_commit.clone());
            }
        }
        let job = crate::job_manager::jobs::git::GitBlameJob::new(
            doc_id as usize,
            repo_root,
            path,
            Some(parent_commit),
        );
        self.job_manager.spawn(job);
    }

    /// `Escape` in a blame buffer: close it and return focus to the linked file.
    pub(super) fn close_git_blame(&mut self) {
        let (doc_id, linked_doc_id) = {
            let doc = self.active_document();
            let linked_doc_id = match &doc.kind {
                BufferKind::GitBlame { linked_doc_id, .. } => *linked_doc_id,
                _ => return,
            };
            (doc.id, linked_doc_id)
        };
        if let Err(e) = self.remove_document(doc_id) {
            self.state.handle_error(e);
            return;
        }
        if self.document_manager.get_document(linked_doc_id).is_some() {
            self.split_tree.set_focused_document(linked_doc_id);
            let _ = self.document_manager.switch_to_document(linked_doc_id);
        }
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Dispatch a `git_blame:*` `Action::Buffer` id.
    pub(super) fn handle_git_blame_buffer_action(&mut self, id: &str) {
        match id {
            "git_blame:walk_back" => self.git_blame_walk_back(),
            "git_blame:close" => self.close_git_blame(),
            _ => {}
        }
    }
}
