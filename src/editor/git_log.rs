//! Git log browser: open, populate, and inline `git show` expansion
//! (Phase 8). Pure read/navigate;  no `:w` reconciliation.

use super::Editor;
use crate::document::BufferKind;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// Open a git log browser, repo-wide (`path: None`) or scoped to one file's history (`path: Some`). `repo_root` is caller-supplied, never re-derived from "whatever's currently active".
    pub fn open_git_log(
        &mut self,
        repo_root: std::path::PathBuf,
        path: Option<std::path::PathBuf>,
    ) -> Option<crate::document::DocumentId> {
        self.open_git_log_scoped(repo_root, path)
    }

    /// `:Git show`: open a repo-wide log browser, then expand HEAD's entry once the commit list populates; the closest structured equivalent to a bare `git show`.
    pub(super) fn open_git_log_expand_head(&mut self, repo_root: std::path::PathBuf) {
        if let Some(id) = self.open_git_log_scoped(repo_root, None) {
            self.pending_git_log_expand_head.insert(id);
        }
    }

    fn open_git_log_scoped(
        &mut self,
        repo_root: std::path::PathBuf,
        path: Option<std::path::PathBuf>,
    ) -> Option<crate::document::DocumentId> {
        let id = self.document_manager.next_id();
        let doc = match crate::document::Document::new_git_log(id, repo_root.clone(), path.clone())
        {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return None;
            }
        };
        self.document_manager.add_document(doc);
        if let Err(e) = self.document_manager.switch_to_document(id) {
            self.state.handle_error(e);
            return None;
        }
        self.split_tree.set_focused_document(id);

        let job = crate::job_manager::jobs::git::GitLogJob::new(id as usize, repo_root, path);
        self.job_manager.spawn(job);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
        Some(id)
    }

    /// `=`/`<CR>` in a log buffer: toggle inline `git show` expansion for
    /// the commit under the cursor.
    pub(super) fn git_log_toggle_expand(&mut self) {
        let (repo_root, sha, already_expanded) = {
            let doc = self.active_document();
            let repo_root = match doc.git_repo_root() {
                Some(r) => r.to_path_buf(),
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);
            let Some(sha) = doc.annotations.git_log_commit_sha_at_line(line) else {
                return;
            };
            let already_expanded = matches!(
                &doc.kind,
                BufferKind::GitLog { expanded, .. } if expanded.as_deref() == Some(sha.as_str())
            );
            (repo_root, sha, already_expanded)
        };

        let doc_id = self.active_document_id();
        if already_expanded {
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                doc.set_git_log_expanded(None, None);
            }
            let _ = self.force_full_redraw();
            return;
        }

        let job = crate::job_manager::jobs::git::GitShowJob::new(doc_id as usize, repo_root, sha);
        self.job_manager.spawn(job);
    }

    /// Dispatch a `git_log:*` `Action::Buffer` id.
    pub(super) fn handle_git_log_buffer_action(&mut self, id: &str) {
        match id {
            "git_log:toggle_expand" | "git_log:select" => self.git_log_toggle_expand(),
            _ => {}
        }
    }
}
