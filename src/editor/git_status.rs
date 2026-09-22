//! Git status buffer and commit workflow.

use super::Editor;
#[allow(unused_imports)]
use crate::buffer::api::BufferView;
use crate::document::{GitCommitTarget, GitStatusAction};
use crate::error::{ErrorType, RiftError};
use crate::term::TerminalBackend;
use std::path::{Path, PathBuf};

/// Executes `actions` against `repo_root` and returns one error message for each failed operation.
fn execute_git_status_actions(repo_root: &Path, actions: &[GitStatusAction]) -> Vec<String> {
    let mut errors = Vec::new();

    let run = |args: &[&str]| -> Result<(), RiftError> {
        crate::git::run_checked(repo_root, args).map(|_| ())
    };

    for action in actions {
        let result = match action {
            GitStatusAction::Stage(path) => {
                let p = path.to_string_lossy();
                run(&["add", "--", &p])
            }
            GitStatusAction::Unstage(path) => {
                let p = path.to_string_lossy();
                run(&["restore", "--staged", "--", &p])
            }
            GitStatusAction::Discard {
                path,
                orig_path,
                was_untracked,
            } => {
                if *was_untracked {
                    crate::fs_backend::backend().delete_recursive(&repo_root.join(path))
                } else {
                    let p = path.to_string_lossy();
                    let mut res = run(&["restore", "--staged", "--worktree", "--", &p]);
                    if let Some(orig) = orig_path {
                        let orig_str = orig.to_string_lossy();
                        let orig_res = run(&["restore", "--staged", "--worktree", "--", &orig_str]);
                        if res.is_ok() {
                            res = orig_res;
                        }
                    }
                    res
                }
            }
            GitStatusAction::StageHunk {
                path,
                hunk,
                is_new_file,
            } => crate::git::apply::stage_hunk(
                repo_root,
                &path.to_string_lossy(),
                hunk,
                *is_new_file,
            ),
            GitStatusAction::UnstageHunk { path, hunk } => {
                crate::git::apply::unstage_hunk(repo_root, &path.to_string_lossy(), hunk)
            }
            GitStatusAction::DiscardHunk {
                path,
                hunk,
                staged_side,
            } => {
                let p = path.to_string_lossy();
                if *staged_side {
                    let cached = crate::git::apply::unstage_hunk(repo_root, &p, hunk);
                    let worktree = crate::git::apply::discard_hunk_worktree(repo_root, &p, hunk);
                    cached.and(worktree)
                } else {
                    crate::git::apply::discard_hunk_worktree(repo_root, &p, hunk)
                }
            }
        };
        if let Err(e) = result {
            errors.push(e.message);
        }
    }

    errors
}

impl<T: TerminalBackend> Editor<T> {
    /// Resolve the git repository root to operate on: the active buffer's own repo root if it's already a git buffer, else discovered from the active file's directory (falling back to cwd), mirroring `OpenExplorer`'s target-path resolution.
    pub(super) fn resolve_git_repo_root(&mut self) -> Result<PathBuf, RiftError> {
        if let Some(root) = self.active_document().git_repo_root() {
            return Ok(root.to_path_buf());
        }
        let base_dir = {
            let doc = self.active_document();
            if let Some(path) = doc.directory_path() {
                path.clone()
            } else {
                doc.path()
                    .map(|p| {
                        if p.is_dir() {
                            p.to_path_buf()
                        } else {
                            p.parent().unwrap_or(p).to_path_buf()
                        }
                    })
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            }
        };
        crate::git::discover_repo(&base_dir).map(|p| p.root)
    }

    /// `<Space>gs` opens a GitStatus buffer in the active window. Open a split first to create another instance.
    pub fn open_git_status(&mut self) {
        self.open_git_status_inner();
    }

    /// `:Git diff`/`:Git diff --cached`: open status, and once the async snapshot populates, expand every hunk in the given section (`true` = staged); the closest structured equivalent to a bare `git diff`.
    pub(super) fn open_git_status_expand_all(&mut self, staged_side: bool) {
        if let Some(id) = self.open_git_status_inner() {
            self.pending_git_status_expand_all.insert(id, staged_side);
        }
    }

    fn open_git_status_inner(&mut self) -> Option<crate::document::DocumentId> {
        let repo_root = match self.resolve_git_repo_root() {
            Ok(root) => root,
            Err(e) => {
                self.state.handle_error(e);
                return None;
            }
        };

        let id = self.document_manager.next_id();
        let doc = match crate::document::Document::new_git_status(id, repo_root.clone()) {
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

        let job = crate::job_manager::jobs::git::GitStatusJob::new(id as usize, repo_root);
        self.job_manager.spawn(job);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
        Some(id)
    }

    /// After a status snapshot arrives, expands every entry in the requested section.
    pub(super) fn spawn_expand_all_git_status_hunks(
        &mut self,
        doc_id: crate::document::DocumentId,
        staged_side: bool,
    ) {
        let Some(doc) = self.document_manager.get_document(doc_id) else {
            return;
        };
        if !doc.is_git_status() {
            return;
        }
        let Some(repo_root) = doc.git_repo_root().map(Path::to_path_buf) else {
            return;
        };
        let Some(snapshot) = doc.git_status_snapshot() else {
            return;
        };
        let paths: Vec<PathBuf> = snapshot
            .entries
            .iter()
            .filter(|e| {
                if staged_side {
                    e.is_staged()
                } else {
                    e.is_unstaged()
                }
            })
            .map(|e| e.path.clone())
            .collect();
        for path in paths {
            let job = crate::job_manager::jobs::git::GitDiffJob::new(
                doc_id as usize,
                repo_root.clone(),
                path,
                staged_side,
                false,
            );
            self.job_manager.spawn(job);
        }
    }

    /// Refresh every open `GitStatus` buffer for `repo_root` (after a commit,
    /// fixup, or a stage/unstage/discard fast-path action).
    pub(super) fn refresh_git_status_buffers_for(&mut self, repo_root: &Path) {
        let doc_ids: Vec<crate::document::DocumentId> = self
            .document_manager
            .documents_iter()
            .filter(|d| d.is_git_status() && d.git_repo_root() == Some(repo_root))
            .map(|d| d.id)
            .collect();
        for doc_id in doc_ids {
            let job = crate::job_manager::jobs::git::GitStatusJob::new(
                doc_id as usize,
                repo_root.to_path_buf(),
            );
            self.job_manager.spawn(job);
        }
    }

    /// Stage/unstage/discard whatever's under the cursor directly (fugitive's `s`/`u`/`-`/`X` fast paths); the buffer is read-only, so this is the only way its state changes. Granularity depends on where the cursor is: a single `+`/`-` diff line stages/unstages/discards just that line (synthesizing a sub-patch via.
    pub(super) fn git_status_cursor_action(&mut self, verb: &str) {
        let mut discard_untracked_hunk_blocked = false;
        let (repo_root, action) = {
            let doc = self.active_document();
            let repo_root = match doc.git_repo_root() {
                Some(r) => r.to_path_buf(),
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);

            let action = if let Some((path, staged_side, hunk_index, line_index)) =
                doc.annotations.git_hunk_line_at_line(line)
            {
                let path = PathBuf::from(path);
                let Some(hunk) = doc.git_status_hunk(&path, staged_side, hunk_index) else {
                    return;
                };
                let is_new_file = doc.is_git_status_entry_untracked(&path);
                let selected = crate::git::diff::diff_line_change_block(&hunk, line_index);
                let hunk = crate::git::diff::filter_hunk_to_lines(&hunk, &selected);
                match verb {
                    "stage" | "toggle" if !staged_side => Some(GitStatusAction::StageHunk {
                        path,
                        hunk,
                        is_new_file,
                    }),
                    "unstage" | "toggle" if staged_side => {
                        Some(GitStatusAction::UnstageHunk { path, hunk })
                    }
                    "discard" if !is_new_file => Some(GitStatusAction::DiscardHunk {
                        path,
                        hunk,
                        staged_side,
                    }),
                    "discard" => {
                        discard_untracked_hunk_blocked = true;
                        None
                    }
                    _ => None,
                }
            } else if let Some((path, staged_side, hunk_index)) =
                doc.annotations.git_hunk_at_line(line)
            {
                let path = PathBuf::from(path);
                let Some(hunk) = doc.git_status_hunk(&path, staged_side, hunk_index) else {
                    return;
                };
                let is_new_file = doc.is_git_status_entry_untracked(&path);
                match verb {
                    "stage" | "toggle" if !staged_side => Some(GitStatusAction::StageHunk {
                        path,
                        hunk,
                        is_new_file,
                    }),
                    "unstage" | "toggle" if staged_side => {
                        Some(GitStatusAction::UnstageHunk { path, hunk })
                    }
                    "discard" if !is_new_file => Some(GitStatusAction::DiscardHunk {
                        path,
                        hunk,
                        staged_side,
                    }),
                    "discard" => {
                        discard_untracked_hunk_blocked = true;
                        None
                    }
                    _ => None,
                }
            } else if let Some((path, section, orig_path)) =
                doc.annotations.git_status_entry_at_line(line)
            {
                let path = PathBuf::from(path);
                let was_untracked = section == "untracked";
                let is_staged = section == "staged";
                if section == "unmerged" {
                    None
                } else {
                    match verb {
                        "stage" if !is_staged => Some(GitStatusAction::Stage(path)),
                        "unstage" if is_staged => Some(GitStatusAction::Unstage(path)),
                        "toggle" if is_staged => Some(GitStatusAction::Unstage(path)),
                        "toggle" if !is_staged => Some(GitStatusAction::Stage(path)),
                        "discard" => Some(GitStatusAction::Discard {
                            path,
                            orig_path: orig_path.map(PathBuf::from),
                            was_untracked,
                        }),
                        _ => None,
                    }
                }
            } else {
                None
            };
            (repo_root, action)
        };

        if discard_untracked_hunk_blocked {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "Discarding part of an untracked file isn't supported yet — X the whole file instead".to_string(),
            );
        }
        let Some(action) = action else { return };
        let errors = execute_git_status_actions(&repo_root, std::slice::from_ref(&action));
        for err in errors {
            self.state
                .notify(crate::notification::NotificationType::Error, err);
        }
        self.refresh_git_status_buffers_for(&repo_root);
    }

    /// `=`: toggle inline hunk expansion for the entry (or the entry owning
    /// the hunk) under the cursor.
    pub(super) fn git_status_toggle_expand(&mut self) {
        let doc_id = self.active_document_id();
        let (repo_root, target) = {
            let doc = self.active_document();
            let repo_root = match doc.git_repo_root() {
                Some(r) => r.to_path_buf(),
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);

            let target = if let Some((path, staged_side, _)) =
                doc.annotations.git_hunk_at_line(line)
            {
                Some((PathBuf::from(path), staged_side, false))
            } else if let Some((path, section, _)) = doc.annotations.git_status_entry_at_line(line)
            {
                match section.as_str() {
                    "staged" => Some((PathBuf::from(path), true, false)),
                    "unstaged" => Some((PathBuf::from(path), false, false)),
                    "untracked" => Some((PathBuf::from(path), false, true)),
                    _ => {
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            "Only staged/unstaged/untracked entries can be expanded".to_string(),
                        );
                        None
                    }
                }
            } else {
                None
            };
            (repo_root, target)
        };

        let Some((path, staged_side, untracked)) = target else {
            return;
        };

        let already_expanded = self
            .document_manager
            .get_document(doc_id)
            .is_some_and(|d| d.is_git_status_expanded(&path, staged_side));

        if already_expanded {
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                doc.collapse_git_status_entry(&path, staged_side);
            }
            let _ = self.force_full_redraw();
            return;
        }

        let job = crate::job_manager::jobs::git::GitDiffJob::new(
            doc_id as usize,
            repo_root,
            path,
            staged_side,
            untracked,
        );
        self.job_manager.spawn(job);
    }

    /// `]c`/`[c`: move the cursor to the next/previous expanded hunk header.
    pub(super) fn git_status_navigate_hunk(&mut self, forward: bool) {
        let doc = self.active_document();
        let cursor = doc.buffer.cursor();
        let current_line = doc.buffer.line_index.get_line_at(cursor);
        let hunks = doc.annotations.git_hunks_by_line();
        if hunks.is_empty() {
            return;
        }
        let target_line = if forward {
            hunks
                .iter()
                .map(|(l, ..)| *l)
                .find(|&l| l > current_line)
                .or_else(|| hunks.first().map(|(l, ..)| *l))
        } else {
            hunks
                .iter()
                .map(|(l, ..)| *l)
                .rev()
                .find(|&l| l < current_line)
                .or_else(|| hunks.last().map(|(l, ..)| *l))
        };
        let Some(target_line) = target_line else {
            return;
        };
        let doc = self.active_document();
        if let Some(start) = doc.buffer.line_index.get_start(target_line) {
            let _ = doc.buffer.set_cursor(start);
        }
    }

    /// `<CR>` on a status-buffer entry line: open that file; on a hunk
    /// header, toggle its expansion; on the `HEAD` summary line, open Log.
    pub(super) fn git_status_select(&mut self) {
        enum Selection {
            Hunk,
            Head,
            File(PathBuf),
        }
        let (selection, repo_root) = {
            let doc = self.active_document();
            let repo_root = match doc.git_repo_root() {
                Some(r) => r.to_path_buf(),
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);
            let selection = if doc.annotations.git_hunk_at_line(line).is_some() {
                Some(Selection::Hunk)
            } else if doc.annotations.is_git_status_head_at_line(line) {
                Some(Selection::Head)
            } else if let Some((path, ..)) = doc.annotations.git_status_entry_at_line(line) {
                Some(Selection::File(repo_root.join(path)))
            } else {
                None
            };
            (selection, repo_root)
        };
        match selection {
            Some(Selection::Hunk) => self.git_status_toggle_expand(),
            Some(Selection::Head) => {
                self.open_git_log(repo_root, None);
            }
            Some(Selection::File(path)) => {
                if let Err(e) = self.open_file(Some(path.display().to_string()), false) {
                    self.state.handle_error(e);
                }
            }
            None => {}
        }
    }

    /// `b` in the status buffer: blame the file under the cursor.
    pub(super) fn git_status_blame_cursor(&mut self) {
        if let Some((path, repo_root)) = self.resolve_git_blame_target(None) {
            self.open_git_blame(path, repo_root);
        }
    }

    /// `r` in the status buffer: start a rebase onto the current branch's
    /// configured upstream (old `<Space>gr` behavior, now scoped to Status).
    pub(super) fn git_status_rebase_cursor(&mut self) {
        let repo_root = match self.active_document().git_repo_root() {
            Some(r) => r.to_path_buf(),
            None => return,
        };
        self.open_git_rebase(repo_root);
    }

    /// Dispatches a `git_status:*` `Action::Buffer` id.
    pub(super) fn handle_git_status_buffer_action(&mut self, id: &str) {
        match id {
            "git_status:stage" => self.git_status_cursor_action("stage"),
            "git_status:unstage" => self.git_status_cursor_action("unstage"),
            "git_status:toggle_stage" => self.git_status_cursor_action("toggle"),
            "git_status:discard" => self.git_status_cursor_action("discard"),
            "git_status:toggle_expand" => self.git_status_toggle_expand(),
            "git_status:next_hunk" => self.git_status_navigate_hunk(true),
            "git_status:prev_hunk" => self.git_status_navigate_hunk(false),
            "git_status:select" => self.git_status_select(),
            "git_status:blame" => self.git_status_blame_cursor(),
            "git_status:rebase" => self.git_status_rebase_cursor(),
            _ => {}
        }
    }

    /// `cc` in the status buffer: open an empty commit message buffer.
    pub fn open_git_commit_new(&mut self) {
        self.open_git_commit_message(GitCommitTarget::New, String::new());
    }

    /// `ca`/`cw` in the status buffer: open a commit message buffer prefilled
    /// with HEAD's message, amending on save.
    pub fn open_git_commit_amend(&mut self) {
        let repo_root = match self.resolve_git_repo_root() {
            Ok(root) => root,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        let message = crate::git::run_checked(&repo_root, &["log", "-1", "--format=%B", "HEAD"])
            .unwrap_or_default();
        self.open_git_commit_message(GitCommitTarget::Amend, message);
    }

    /// `cf` in the status buffer: commit currently-staged changes as a fixup
    /// of HEAD (`git commit --fixup=HEAD`);  no message buffer needed.
    pub fn run_git_commit_fixup(&mut self) {
        let repo_root = match self.resolve_git_repo_root() {
            Ok(root) => root,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        match crate::git::run_checked(&repo_root, &["commit", "--fixup=HEAD", "--quiet"]) {
            Ok(_) => {
                self.state.notify(
                    crate::notification::NotificationType::Info,
                    "Committed fixup! HEAD".to_string(),
                );
            }
            Err(e) => self.state.handle_error(e),
        }
        self.refresh_git_status_buffers_for(&repo_root);
    }

    pub(super) fn open_git_commit_message(
        &mut self,
        target: GitCommitTarget,
        initial_message: String,
    ) {
        let repo_root = match self.resolve_git_repo_root() {
            Ok(root) => root,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        let id = self.document_manager.next_id();
        let doc = match crate::document::Document::new_git_commit_message(
            id,
            repo_root,
            target,
            &initial_message,
        ) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        self.document_manager.add_document(doc);
        if let Err(e) = self.document_manager.switch_to_document(id) {
            self.state.handle_error(e);
            return;
        }
        self.split_tree.set_focused_document(id);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// `:w`/`:wq` on a `GitCommitMessage` buffer: commit (or amend) with the
    /// buffer's text, then close it.
    pub(super) fn apply_git_commit_message(&mut self) {
        let (repo_root, target, message, doc_id) = {
            let doc = match self.document_manager.active_document() {
                Some(d) => d,
                None => return,
            };
            if !doc.is_git_commit_message() {
                return;
            }
            let (Some(repo_root), Some(target)) = (
                doc.git_repo_root().map(Path::to_path_buf),
                doc.git_commit_target().cloned(),
            ) else {
                return;
            };
            let message: String = doc
                .buffer
                .chars(0..doc.buffer.len())
                .map(|c| c.to_char_lossy())
                .collect();
            (repo_root, target, message, doc.id)
        };

        if let GitCommitTarget::RebasePlanReword { rebase_doc_id, sha } = target {
            self.state.clear_command_line();
            if let Err(e) = self.remove_document(doc_id) {
                self.state.handle_error(e);
                return;
            }
            self.apply_rebase_plan_reword(rebase_doc_id, &sha, message);
            return;
        }
        if message.trim().is_empty() {
            self.state.notify(
                crate::notification::NotificationType::Error,
                "Aborting commit due to empty commit message".to_string(),
            );
            return;
        }

        let tmp_path = match crate::git::git_dir(&repo_root) {
            Ok(dir) => dir.join("RIFT_COMMIT_EDITMSG"),
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        if let Err(e) = std::fs::write(&tmp_path, &message) {
            self.state.handle_error(RiftError::new(
                ErrorType::Io,
                "GIT_COMMIT_MSG_WRITE_FAILED",
                e.to_string(),
            ));
            return;
        }

        let tmp_path_str = tmp_path.to_string_lossy().into_owned();
        let mut args = vec!["commit", "-F", tmp_path_str.as_str(), "--quiet"];
        let is_reword = matches!(target, GitCommitTarget::RebaseReword { .. });
        if target == GitCommitTarget::Amend || is_reword {
            args.push("--amend");
        }
        let result = crate::git::run_checked(&repo_root, &args);
        let _ = std::fs::remove_file(&tmp_path);

        match result {
            Ok(_) => {
                self.state.clear_command_line();
                if let Err(e) = self.remove_document(doc_id) {
                    self.state.handle_error(e);
                    return;
                }
                let paused_edit_rebase = if target == GitCommitTarget::Amend {
                    self.find_paused_edit_rebase(&repo_root)
                } else {
                    None
                };
                if let GitCommitTarget::RebaseReword { rebase_doc_id } = target {
                    self.resume_rebase_after_reword(rebase_doc_id, &repo_root);
                } else if let Some((rebase_doc_id, remaining)) = paused_edit_rebase {
                    self.state.notify(
                        crate::notification::NotificationType::Info,
                        "Amended — resuming rebase".to_string(),
                    );
                    self.resume_rebase_after_edit_amend(rebase_doc_id, &repo_root, remaining);
                } else {
                    self.state.notify(
                        crate::notification::NotificationType::Info,
                        "Committed".to_string(),
                    );
                    self.refresh_git_status_buffers_for(&repo_root);
                }
            }
            Err(e) => self.state.handle_error(e),
        }
    }

    /// `:Git <args>` escape hatch: run an arbitrary git subcommand and show its output. View-only; no `--no-ext-diff`, so a configured `diff.external` etc. is respected for anything that falls through. Fugitive convention: bare `:Git`/`:G` opens the status buffer, bare `:Git log` opens the log browser, bare `:Git.
    pub fn run_git_command(&mut self, args: String) {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            self.open_git_status();
            return;
        }
        if trimmed == "log" {
            let repo_root = match self.resolve_git_repo_root() {
                Ok(root) => root,
                Err(e) => {
                    self.state.handle_error(e);
                    return;
                }
            };
            self.open_git_log(repo_root, None);
            return;
        }
        if trimmed == "diff" {
            self.open_git_status_expand_all(false);
            return;
        }
        if trimmed == "diff --cached" || trimmed == "diff --staged" {
            self.open_git_status_expand_all(true);
            return;
        }
        // `blame <path>` opens the structured view. Flag arguments use the raw command output path.
        if trimmed == "blame"
            || (trimmed.starts_with("blame ") && !trimmed[6..].trim_start().starts_with('-'))
        {
            let path_arg = trimmed.strip_prefix("blame").unwrap().trim();
            let arg = if path_arg.is_empty() {
                None
            } else {
                Some(path_arg)
            };
            if let Some((path, repo_root)) = self.resolve_git_blame_target(arg) {
                self.open_git_blame(path, repo_root);
            }
            return;
        }
        if trimmed == "show" {
            let repo_root = match self.resolve_git_repo_root() {
                Ok(root) => root,
                Err(e) => {
                    self.state.handle_error(e);
                    return;
                }
            };
            self.open_git_log_expand_head(repo_root);
            return;
        }
        let repo_root = match self.resolve_git_repo_root() {
            Ok(root) => root,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        let origin_doc_id = self.active_document_id();
        let job = crate::job_manager::jobs::git::GitCommandJob::new(
            origin_doc_id as usize,
            repo_root,
            args,
        );
        self.job_manager.spawn(job);
    }

    /// Resolve `(path, repo_root)` for a blame request; an explicit `arg` path wins; otherwise the Status buffer's cursor file if that's the active buffer; otherwise the active file buffer's own path. This is the one place that resolution lives: `open_git_blame` itself takes plain explicit arguments and never.
    fn resolve_git_blame_target(&mut self, arg: Option<&str>) -> Option<(PathBuf, PathBuf)> {
        if let Some(arg) = arg {
            let repo_root = match self.resolve_git_repo_root() {
                Ok(root) => root,
                Err(e) => {
                    self.state.handle_error(e);
                    return None;
                }
            };
            return Some((repo_root.join(arg), repo_root));
        }
        let doc = self.active_document();
        if doc.is_git_status() {
            let repo_root = doc.git_repo_root()?.to_path_buf();
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);
            let (path, ..) = doc.annotations.git_status_entry_at_line(line)?;
            return Some((repo_root.join(path), repo_root));
        }
        if let Some(path) = doc.path().map(|p| p.to_path_buf()) {
            let base_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            return match crate::git::discover_repo(&base_dir) {
                Ok(r) => Some((path, r.root)),
                Err(e) => {
                    self.state.handle_error(e);
                    None
                }
            };
        }
        self.state.notify(
            crate::notification::NotificationType::Error,
            "No file to blame".to_string(),
        );
        None
    }

    /// Show a `GitCommandJob`'s output: a scratch buffer for anything
    /// non-trivial, or just a notification for a short one-line result.
    pub(super) fn show_git_command_result(&mut self, args: &str, output: String, success: bool) {
        let trimmed = output.trim();
        if trimmed.lines().count() <= 1 && trimmed.len() <= 200 {
            let kind = if success {
                crate::notification::NotificationType::Info
            } else {
                crate::notification::NotificationType::Error
            };
            self.state.notify(
                kind,
                if trimmed.is_empty() {
                    format!("git {args}: done")
                } else {
                    trimmed.to_string()
                },
            );
            return;
        }

        let id = self.document_manager.next_id();
        let lines: Vec<String> = output.lines().map(|l| l.to_string()).collect();
        let doc = match crate::document::Document::new_scratch(id, format!("[Git: {args}]"), &lines)
        {
            Ok(mut d) => {
                d.set_read_only(true);
                d
            }
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        self.document_manager.add_document(doc);
        if let Err(e) = self.document_manager.switch_to_document(id) {
            self.state.handle_error(e);
            return;
        }
        self.split_tree.set_focused_document(id);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// `g?` in a git status/log/blame/rebase-todo buffer: open a read-only
    /// scratch buffer listing that buffer's key reference.
    pub(super) fn open_git_help(&mut self) {
        let Some(lines) = self.active_document().help_lines().map(<[String]>::to_vec) else {
            return;
        };
        let id = self.document_manager.next_id();
        let doc = match crate::document::Document::new_scratch(id, "[Git Help]".to_string(), &lines)
        {
            Ok(mut d) => {
                d.set_read_only(true);
                d
            }
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        self.document_manager.add_document(doc);
        if let Err(e) = self.document_manager.switch_to_document(id) {
            self.state.handle_error(e);
            return;
        }
        self.split_tree.set_focused_document(id);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }
}
