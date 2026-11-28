use super::Editor;
use crate::document::runtime::BufferKindId;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// `=`/`<CR>` in a log buffer: toggle inline `git show` expansion for
    /// the commit under the cursor.
    pub(super) fn git_log_toggle_expand(&mut self) {
        let (repo_root, sha, already_expanded) = {
            let doc = self.active_document();
            if doc.buffer_kind_id() != BufferKindId::GIT_LOG {
                return;
            }
            let repo_root = match doc.git_repo_root() {
                Some(r) => r.to_path_buf(),
                None => return,
            };
            let cursor = doc.buffer().cursor();
            let line = doc.buffer().line_index.get_line_at(cursor);
            let Some(sha) = doc.annotations.git_log_commit_sha_at_line(line) else {
                return;
            };
            let already_expanded = doc.git_log_expanded() == Some(sha.as_str());
            (repo_root, sha, already_expanded)
        };

        let doc_id = self.active_document_id();
        if already_expanded {
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                doc.set_git_log_expanded(None, None);
            }
            return;
        }
        self.fetch_git_commit_body(doc_id, repo_root, sha);
    }
}
