//! Git blame view: open, populate, and walk-back navigation. Read-only; no `:w` reconciliation.

use super::Editor;
use crate::document::BufferKindId;
use crate::term::TerminalBackend;

const BLAME_PANE_WIDTH_RATIO: f64 = 0.33;

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
            .split_before_with_ratio(
                crate::split::tree::SplitDirection::Vertical,
                focused,
                blame_doc_id,
                rows,
                cols,
                BLAME_PANE_WIDTH_RATIO,
            )
            .expect("focused window is always a valid leaf");
        self.split_tree.set_focus(blame_win_id);
        let _ = self.document_manager.switch_to_document(blame_doc_id);
        if let Some(doc) = self.document_manager.get_document_mut(blame_doc_id) {
            if doc.buffer_kind_id() == BufferKindId::GIT_BLAME {
                doc.set_git_blame_linked_window_id(focused);
            }
        }

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

    pub(super) fn git_blame_walk_back(&mut self) {
        let (repo_root, path, sha, linked_doc_id, linked_window_id) = {
            let doc = self.active_document();
            if doc.buffer_kind_id() != BufferKindId::GIT_BLAME {
                return;
            }
            let (Some(repo_root), Some(path), Some(linked_doc_id), Some(linked_window_id)) = (
                doc.git_repo_root().map(std::path::Path::to_path_buf),
                doc.git_blame_path().map(std::path::Path::to_path_buf),
                doc.git_blame_linked_doc_id(),
                doc.git_blame_linked_window_id(),
            ) else {
                return;
            };
            let line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
            let Some(sha) = doc.annotations.git_blame_sha_at_line(line) else {
                return;
            };
            (repo_root, path, sha, linked_doc_id, linked_window_id)
        };
        let parent_commit = format!("{sha}^");
        if !crate::git::run(
            &repo_root,
            &["rev-parse", "--verify", "--quiet", &parent_commit],
        )
        .map(|out| out.success)
        .unwrap_or(false)
        {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "This commit introduced the line — nothing earlier to walk back to".to_string(),
            );
            return;
        }
        if !self.set_git_blame_source(
            &repo_root,
            &path,
            linked_doc_id,
            linked_window_id,
            Some(&parent_commit),
        ) {
            return;
        }
        let doc_id = self.active_document_id();
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            if doc.buffer_kind_id() == BufferKindId::GIT_BLAME {
                let current_commit = doc.git_blame_at_commit().map(str::to_string);
                doc.push_git_blame_history(current_commit);
                doc.set_git_blame_at_commit(Some(parent_commit.clone()));
            }
        }
        self.job_manager
            .spawn(crate::job_manager::jobs::git::GitBlameJob::new(
                doc_id as usize,
                repo_root,
                path,
                Some(parent_commit),
            ));
    }

    pub(super) fn git_blame_walk_forward(&mut self) {
        let (repo_root, path, linked_doc_id, linked_window_id, previous) = {
            let doc = self.active_document();
            if doc.buffer_kind_id() != BufferKindId::GIT_BLAME {
                return;
            }
            let (Some(repo_root), Some(path), Some(linked_doc_id), Some(linked_window_id)) = (
                doc.git_repo_root().map(std::path::Path::to_path_buf),
                doc.git_blame_path().map(std::path::Path::to_path_buf),
                doc.git_blame_linked_doc_id(),
                doc.git_blame_linked_window_id(),
            ) else {
                return;
            };
            let Some(previous) = doc.git_blame_history().and_then(|h| h.last().cloned()) else {
                return;
            };
            (repo_root, path, linked_doc_id, linked_window_id, previous)
        };
        if !self.set_git_blame_source(
            &repo_root,
            &path,
            linked_doc_id,
            linked_window_id,
            previous.as_deref(),
        ) {
            return;
        }
        let doc_id = self.active_document_id();
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            if doc.buffer_kind_id() == BufferKindId::GIT_BLAME {
                if let Some(previous_commit) = doc.pop_git_blame_history() {
                    doc.set_git_blame_at_commit(previous_commit);
                }
            }
        }
        self.job_manager
            .spawn(crate::job_manager::jobs::git::GitBlameJob::new(
                doc_id as usize,
                repo_root,
                path,
                previous,
            ));
    }

    fn set_git_blame_source(
        &mut self,
        repo_root: &std::path::Path,
        path: &std::path::Path,
        linked_doc_id: crate::document::DocumentId,
        linked_window_id: crate::split::window::WindowId,
        commit: Option<&str>,
    ) -> bool {
        let active_doc_id = self.active_document_id();
        let old_doc_id = self
            .split_tree
            .get_window(linked_window_id)
            .map(|window| window.document_id);
        match commit {
            Some(commit) => {
                let root = repo_root.to_string_lossy().replace('\\', "/");
                let root = root.strip_prefix("//?/").unwrap_or(&root);
                let full_path = path.to_string_lossy().replace('\\', "/");
                let full_path = full_path.strip_prefix("//?/").unwrap_or(&full_path);
                let relative = full_path
                    .strip_prefix(root.trim_end_matches('/'))
                    .unwrap_or(full_path)
                    .trim_start_matches('/');
                let spec = format!("{commit}:{relative}");
                let content = match crate::git::run_checked(repo_root, &["cat-file", "blob", &spec])
                {
                    Ok(content) => content,
                    Err(error) => {
                        self.state.handle_error(error);
                        return false;
                    }
                };
                let id = self.document_manager.next_id();
                let mut doc = match crate::document::Document::from_file(id, path) {
                    Ok(doc) => doc,
                    Err(error) => {
                        self.state.handle_error(error);
                        return false;
                    }
                };
                doc.replace_buffer_content(&content);
                doc.set_read_only(true);
                doc.convert_to_scratch(format!(
                    "[Git] {} @ {}",
                    path.display(),
                    &commit[..commit.len().min(8)]
                ));
                self.document_manager.add_private_document(doc);
                self.split_tree.set_window_document(linked_window_id, id);
                if let Some(old_doc_id) = old_doc_id.filter(|id| *id != linked_doc_id) {
                    let _ = self.remove_private_document(old_doc_id);
                }
            }
            None => {
                self.split_tree
                    .set_window_document(linked_window_id, linked_doc_id);
                if let Some(old_doc_id) = old_doc_id.filter(|id| *id != linked_doc_id) {
                    let _ = self.remove_private_document(old_doc_id);
                }
            }
        }
        let _ = self.document_manager.switch_to_document(active_doc_id);
        true
    }

    /// Mirrors the focused pane's cursor into its blame/source partner
    /// before viewports update, since viewports are cursor-driven.
    pub(super) fn sync_git_blame_cursor(&mut self) {
        let line = {
            let window = self.split_tree.focused_window();
            let Some(doc) = self.document_manager.get_document(window.document_id) else {
                return;
            };
            let buffer_line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
            doc.annotations
                .git_blame_source_line_at_line(buffer_line)
                .unwrap_or(buffer_line)
        };

        let Some(target_window_id) = self.git_blame_partner_window_id() else {
            return;
        };
        let Some(target_doc_id) = self
            .split_tree
            .get_window(target_window_id)
            .map(|window| window.document_id)
        else {
            return;
        };
        let Some(target_doc) = self.document_manager.get_document_mut(target_doc_id) else {
            return;
        };
        let target_line = target_doc
            .annotations
            .git_blame_line_for_source_line(line)
            .unwrap_or(line)
            .min(target_doc.buffer.get_total_lines().saturating_sub(1));
        let cursor = target_doc
            .buffer
            .line_index
            .get_start(target_line)
            .unwrap_or(0);
        target_doc
            .buffer
            .set_cursor(cursor)
            .expect("line index must return a valid cursor offset");
        if let Some(window) = self.split_tree.get_window_mut(target_window_id) {
            window.cursor_position = cursor;
        }
    }

    /// The pane linked to the focused Git blame or source window.
    pub(super) fn git_blame_partner_window_id(&self) -> Option<crate::split::window::WindowId> {
        let focused_window_id = self.split_tree.focused_window_id();
        let focused_doc_id = self.split_tree.focused_window().document_id;
        if let Some(linked_window_id) = self
            .document_manager
            .get_document(focused_doc_id)
            .and_then(|doc| doc.git_blame_linked_window_id())
        {
            return Some(linked_window_id);
        }

        self.document_manager
            .documents_iter()
            .find_map(|doc| {
                if doc.buffer_kind_id() == BufferKindId::GIT_BLAME
                    && doc.git_blame_linked_window_id() == Some(focused_window_id)
                {
                    Some(doc.id)
                } else {
                    None
                }
            })
            .and_then(|blame_doc_id| {
                self.split_tree
                    .windows
                    .iter()
                    .find_map(|(window_id, window)| {
                        (window.document_id == blame_doc_id).then_some(*window_id)
                    })
            })
    }
    /// `Escape` in a blame buffer: close it and return focus to the linked file.
    pub(super) fn close_git_blame(&mut self) {
        let (doc_id, linked_doc_id, linked_window_id) = {
            let doc = self.active_document();
            if doc.buffer_kind_id() != BufferKindId::GIT_BLAME {
                return;
            }
            let (Some(linked_doc_id), Some(linked_window_id)) = (
                doc.git_blame_linked_doc_id(),
                doc.git_blame_linked_window_id(),
            ) else {
                return;
            };
            (doc.id, linked_doc_id, linked_window_id)
        };
        self.set_git_blame_source(
            std::path::Path::new(""),
            std::path::Path::new(""),
            linked_doc_id,
            linked_window_id,
            None,
        );
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
            "git_blame:walk_forward" => self.git_blame_walk_forward(),
            "git_blame:close" => self.close_git_blame(),
            _ => {}
        }
    }
}
