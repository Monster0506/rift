//! Executes a rebase plan through cherry-pick, amend, and ref updates. `:w` resumes after edit, reword, or conflict pauses.

use super::Editor;
use crate::document::{DocumentId, GitCommitTarget};
use crate::error::{ErrorType, RiftError};
use crate::git::rebase::{RebasePause, RebaseStep, RebaseVerb};
use crate::notification::NotificationType;
use crate::term::TerminalBackend;
use std::path::{Path, PathBuf};

enum CherryPickOutcome {
    Ok,
    Conflict,
    Err(RiftError),
}

fn cherry_pick_in_progress(repo_root: &Path) -> bool {
    crate::git::git_dir(repo_root)
        .map(|d| d.join("CHERRY_PICK_HEAD").exists())
        .unwrap_or(false)
}

fn cherry_pick(repo_root: &Path, sha: &str, no_commit: bool) -> CherryPickOutcome {
    let mut args = vec!["cherry-pick"];
    if no_commit {
        args.push("--no-commit");
    }
    args.push(sha);
    match crate::git::run(repo_root, &args) {
        Ok(out) if out.success => CherryPickOutcome::Ok,
        Ok(out) if cherry_pick_in_progress(repo_root) => {
            let _ = out;
            CherryPickOutcome::Conflict
        }
        Ok(out) => CherryPickOutcome::Err(RiftError::new(
            ErrorType::Execution,
            "CHERRY_PICK_FAILED",
            out.stderr,
        )),
        Err(e) => CherryPickOutcome::Err(e),
    }
}

fn amend_with_message(repo_root: &Path, message: &str) -> Result<(), RiftError> {
    let tmp = crate::git::git_dir(repo_root)?.join("RIFT_REBASE_MSG");
    std::fs::write(&tmp, message)
        .map_err(|e| RiftError::new(ErrorType::Io, "REBASE_MSG_WRITE_FAILED", e.to_string()))?;
    let result = crate::git::run_checked(
        repo_root,
        &[
            "commit",
            "--amend",
            "-F",
            tmp.to_string_lossy().as_ref(),
            "--quiet",
        ],
    );
    let _ = std::fs::remove_file(&tmp);
    result.map(|_| ())
}

fn amend_keep_message(repo_root: &Path) -> Result<(), RiftError> {
    crate::git::run_checked(repo_root, &["commit", "--amend", "--no-edit", "--quiet"]).map(|_| ())
}

fn commit_message_of(repo_root: &Path, rev: &str) -> String {
    crate::git::run_checked(repo_root, &["log", "-1", "--format=%B", rev]).unwrap_or_default()
}

fn head_sha(repo_root: &Path) -> String {
    crate::git::run_checked(repo_root, &["rev-parse", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Outcome of driving the step executor forward from a given point.
enum RebaseAdvance {
    Completed {
        new_tip: String,
    },
    PausedForEdit {
        remaining: Vec<RebaseStep>,
    },
    PausedForReword {
        message: String,
        remaining: Vec<RebaseStep>,
    },
    Conflict {
        remaining: Vec<RebaseStep>,
    },
    Error(RiftError),
}

/// Execute `steps` in order against the current (already-checked-out) detached HEAD, stopping at the first step that needs a pause. `overrides` are pre-edited full messages (sha -> `<subject>\n\n<body>`) from the todo's `c`/`r` sub-editor: when present, `pick`/`edit`/`reword` amend straight to that text (no.
fn advance_rebase(
    repo_root: &Path,
    mut steps: Vec<RebaseStep>,
    overrides: &std::collections::HashMap<String, String>,
) -> RebaseAdvance {
    loop {
        if steps.is_empty() {
            return RebaseAdvance::Completed {
                new_tip: head_sha(repo_root),
            };
        }
        let step = steps.remove(0);
        let override_message = overrides.get(&step.sha).cloned();
        match step.verb {
            RebaseVerb::Drop => continue,
            RebaseVerb::Pick => match cherry_pick(repo_root, &step.sha, false) {
                CherryPickOutcome::Ok => {
                    if let Some(msg) = override_message {
                        if let Err(e) = amend_with_message(repo_root, &msg) {
                            return RebaseAdvance::Error(e);
                        }
                    }
                    continue;
                }
                CherryPickOutcome::Conflict => return RebaseAdvance::Conflict { remaining: steps },
                CherryPickOutcome::Err(e) => return RebaseAdvance::Error(e),
            },
            RebaseVerb::Edit => match cherry_pick(repo_root, &step.sha, false) {
                CherryPickOutcome::Ok => {
                    if let Some(msg) = override_message {
                        if let Err(e) = amend_with_message(repo_root, &msg) {
                            return RebaseAdvance::Error(e);
                        }
                    }
                    return RebaseAdvance::PausedForEdit { remaining: steps };
                }
                CherryPickOutcome::Conflict => return RebaseAdvance::Conflict { remaining: steps },
                CherryPickOutcome::Err(e) => return RebaseAdvance::Error(e),
            },
            RebaseVerb::Reword => match cherry_pick(repo_root, &step.sha, false) {
                CherryPickOutcome::Ok => {
                    if let Some(msg) = override_message {
                        // Already know the desired message; apply it now, no pause needed. The pause-for-full-message flow is only a fallback for a `reword` nobody pre-edited.
                        if let Err(e) = amend_with_message(repo_root, &msg) {
                            return RebaseAdvance::Error(e);
                        }
                        continue;
                    }
                    let message = commit_message_of(repo_root, "HEAD");
                    return RebaseAdvance::PausedForReword {
                        message,
                        remaining: steps,
                    };
                }
                CherryPickOutcome::Conflict => return RebaseAdvance::Conflict { remaining: steps },
                CherryPickOutcome::Err(e) => return RebaseAdvance::Error(e),
            },
            RebaseVerb::Squash | RebaseVerb::Fixup => {
                match cherry_pick(repo_root, &step.sha, true) {
                    CherryPickOutcome::Ok => {
                        let result = if step.verb == RebaseVerb::Squash {
                            let prev = commit_message_of(repo_root, "HEAD");
                            let incoming = override_message
                                .unwrap_or_else(|| commit_message_of(repo_root, &step.sha));
                            let combined =
                                crate::git::rebase::combine_squash_messages(&prev, &incoming);
                            amend_with_message(repo_root, &combined)
                        } else {
                            amend_keep_message(repo_root)
                        };
                        if let Err(e) = result {
                            return RebaseAdvance::Error(e);
                        }
                        continue;
                    }
                    CherryPickOutcome::Conflict => {
                        return RebaseAdvance::Conflict { remaining: steps }
                    }
                    CherryPickOutcome::Err(e) => return RebaseAdvance::Error(e),
                }
            }
        }
    }
}

impl<T: TerminalBackend> Editor<T> {
    /// `r` in the Status buffer: start a rebase of the current branch onto its configured upstream. No interactive base picker; configure an upstream first (`git branch --set-upstream-to=<ref>`), or use `r` on a commit in the Log buffer to rebase from a specific point in history instead. `repo_root` is.
    pub fn open_git_rebase(&mut self, repo_root: PathBuf) {
        let Some(branch) = self.current_branch_or_notify(&repo_root) else {
            return;
        };
        let base = match crate::git::run_checked(
            &repo_root,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        ) {
            Ok(u) => u.trim().to_string(),
            Err(_) => {
                self.state.notify(
                    NotificationType::Error,
                    "No upstream configured for this branch; set one with `git branch --set-upstream-to=<ref>`"
                        .to_string(),
                );
                return;
            }
        };
        self.start_rebase_from_base(repo_root, branch, base);
    }

    /// `r` on a commit in the Log buffer: start a rebase from that commit onward (base = its parent), for touching up history older than the branch's upstream; reordering/rewording/squashing commits that have already diverged past `@{upstream}` isn't reachable from `r` in the Status buffer alone.
    pub(super) fn git_rebase_from_log_commit(&mut self) {
        let (repo_root, sha) = {
            let doc = self.active_document();
            let Some(repo_root) = doc.git_repo_root().map(Path::to_path_buf) else {
                return;
            };
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);
            let Some(sha) = doc.annotations.git_log_commit_sha_at_line(line) else {
                return;
            };
            (repo_root, sha)
        };
        let Some(branch) = self.current_branch_or_notify(&repo_root) else {
            return;
        };
        let parent_expr = format!("{sha}^");
        if crate::git::run_checked(
            &repo_root,
            &["rev-parse", "--verify", "--quiet", &parent_expr],
        )
        .is_err()
        {
            self.state.notify(
                NotificationType::Error,
                "Cannot rebase from the root commit — it has no parent".to_string(),
            );
            return;
        }
        let base = format!("{}^", &sha[..sha.len().min(8)]);
        self.start_rebase_from_base(repo_root, branch, base);
    }

    /// The current branch, or `None` after notifying if HEAD is detached.
    fn current_branch_or_notify(&mut self, repo_root: &Path) -> Option<String> {
        let branch =
            match crate::git::run_checked(repo_root, &["rev-parse", "--abbrev-ref", "HEAD"]) {
                Ok(b) => b.trim().to_string(),
                Err(e) => {
                    self.state.handle_error(e);
                    return None;
                }
            };
        if branch == "HEAD" {
            self.state.notify(
                NotificationType::Error,
                "Cannot rebase: HEAD is detached".to_string(),
            );
            return None;
        }
        Some(branch)
    }

    /// Build and open the todo for `base..HEAD` on `branch`. `base` is any
    /// revision expression git accepts (an upstream ref, or `<sha>^`).
    fn start_rebase_from_base(&mut self, repo_root: PathBuf, branch: String, base: String) {
        let saved_head = head_sha(&repo_root);
        let log_out = match crate::git::run_checked(
            &repo_root,
            &[
                "log",
                &format!("--format={}", crate::git::log::LOG_FORMAT),
                &format!("{base}..HEAD"),
            ],
        ) {
            Ok(o) => o,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        let commits = crate::git::log::parse_log(&log_out);
        if commits.is_empty() {
            self.state.notify(
                NotificationType::Info,
                format!("Already up to date with {base}"),
            );
            return;
        }
        let steps = crate::git::rebase::initial_todo_from_log(&commits);

        let id = self.document_manager.next_id();
        let doc = match crate::document::Document::new_git_rebase_todo(
            id, repo_root, base, saved_head, branch, &steps,
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

    /// `:w` on a `GitRebaseTodo` buffer: start (if not yet running) or resume (if paused) executing it. Runs the buffer's structured `steps`/`message_overrides` directly; the rendered text is a view, not re-parsed.
    pub(super) fn apply_git_rebase_todo(&mut self) {
        let (repo_root, base, branch, pause, steps, message_overrides) = {
            let doc = match self.document_manager.active_document() {
                Some(d) => d,
                None => return,
            };
            if !doc.is_git_rebase_todo() {
                return;
            }
            let (Some(repo_root), Some(base), Some(branch), Some(steps), Some(message_overrides)) = (
                doc.git_repo_root().map(Path::to_path_buf),
                doc.git_rebase_base().map(str::to_string),
                doc.git_rebase_branch().map(str::to_string),
                doc.git_rebase_steps().map(|s| s.to_vec()),
                doc.git_rebase_message_overrides().cloned(),
            ) else {
                return;
            };
            let pause = doc.git_rebase_pause().cloned();
            (repo_root, base, branch, pause, steps, message_overrides)
        };
        let doc_id = self.active_document_id();

        match pause {
            None => {
                if steps.is_empty() {
                    self.state.notify(
                        NotificationType::Info,
                        "Nothing to rebase (empty todo)".to_string(),
                    );
                    return;
                }
                if let Err(e) =
                    crate::git::run_checked(&repo_root, &["checkout", "--detach", &base])
                {
                    self.state.handle_error(e);
                    return;
                }
                self.run_rebase_steps(doc_id, &repo_root, &branch, steps, &message_overrides);
            }
            Some(RebasePause::Edit { remaining }) => {
                self.run_rebase_steps(doc_id, &repo_root, &branch, remaining, &message_overrides);
            }
            Some(RebasePause::Conflict { remaining }) => {
                match crate::git::run(&repo_root, &["cherry-pick", "--continue"]) {
                    Ok(out) if out.success => {
                        self.run_rebase_steps(
                            doc_id,
                            &repo_root,
                            &branch,
                            remaining,
                            &message_overrides,
                        );
                    }
                    _ if cherry_pick_in_progress(&repo_root) => {
                        self.state.notify(
                            NotificationType::Warning,
                            "Still conflicted — resolve remaining conflicts (see <Space>gs), stage them, then :w again"
                                .to_string(),
                        );
                    }
                    Ok(out) => {
                        self.state.handle_error(RiftError::new(
                            ErrorType::Execution,
                            "CHERRY_PICK_CONTINUE_FAILED",
                            out.stderr,
                        ));
                    }
                    Err(e) => self.state.handle_error(e),
                }
            }
            Some(RebasePause::AwaitingReword { .. }) => {
                self.state.notify(
                    NotificationType::Info,
                    "Finish the reword commit message buffer first".to_string(),
                );
            }
        }
    }

    /// Called once the reword commit-message buffer for `rebase_doc_id` has
    /// been amended successfully: resumes that rebase's remaining steps.
    pub(super) fn resume_rebase_after_reword(
        &mut self,
        rebase_doc_id: DocumentId,
        repo_root: &Path,
    ) {
        let (branch, remaining, message_overrides) = {
            let Some(doc) = self.document_manager.get_document(rebase_doc_id) else {
                return;
            };
            if !doc.is_git_rebase_todo() {
                return;
            }
            let Some(RebasePause::AwaitingReword { remaining }) = doc.git_rebase_pause() else {
                return;
            };
            let (Some(branch), Some(message_overrides)) = (
                doc.git_rebase_branch().map(str::to_string),
                doc.git_rebase_message_overrides().cloned(),
            ) else {
                return;
            };
            (branch, remaining.clone(), message_overrides)
        };
        self.run_rebase_steps(
            rebase_doc_id,
            repo_root,
            &branch,
            remaining,
            &message_overrides,
        );
    }

    /// Find an open `GitRebaseTodo` for `repo_root` currently paused for `edit`, if any. Lets `ca`/`cw` in the Status buffer auto-resume the rebase right after amending the paused commit; finishing an edit pause (make changes, stage hunks, amend) never needs a separate trip back to `:w` the todo buffer.
    pub(super) fn find_paused_edit_rebase(
        &self,
        repo_root: &Path,
    ) -> Option<(DocumentId, Vec<RebaseStep>)> {
        self.document_manager.documents_iter().find_map(|d| {
            if d.is_git_rebase_todo() && d.git_repo_root() == Some(repo_root) {
                if let Some(RebasePause::Edit { remaining }) = d.git_rebase_pause() {
                    return Some((d.id, remaining.clone()));
                }
            }
            None
        })
    }

    /// Called once `ca`/`cw` successfully amends HEAD while `rebase_doc_id`
    /// is paused for `edit`: resumes its remaining steps immediately.
    pub(super) fn resume_rebase_after_edit_amend(
        &mut self,
        rebase_doc_id: DocumentId,
        repo_root: &Path,
        remaining: Vec<RebaseStep>,
    ) {
        let Some((branch, message_overrides)) = self
            .document_manager
            .get_document(rebase_doc_id)
            .and_then(|doc| {
                if !doc.is_git_rebase_todo() {
                    return None;
                }
                let branch = doc.git_rebase_branch()?.to_string();
                let message_overrides = doc.git_rebase_message_overrides()?.clone();
                Some((branch, message_overrides))
            })
        else {
            return;
        };
        self.run_rebase_steps(
            rebase_doc_id,
            repo_root,
            &branch,
            remaining,
            &message_overrides,
        );
    }

    /// Drive the executor from `steps`, applying whatever it decides
    /// (complete the rebase, pause, or surface an error) to `doc_id`.
    fn run_rebase_steps(
        &mut self,
        doc_id: DocumentId,
        repo_root: &Path,
        branch: &str,
        steps: Vec<RebaseStep>,
        message_overrides: &std::collections::HashMap<String, String>,
    ) {
        match advance_rebase(repo_root, steps, message_overrides) {
            RebaseAdvance::Completed { new_tip } => {
                let branch_ref = format!("refs/heads/{branch}");
                if let Err(e) =
                    crate::git::run_checked(repo_root, &["update-ref", &branch_ref, &new_tip])
                {
                    self.state.handle_error(e);
                    return;
                }
                if let Err(e) = crate::git::run_checked(repo_root, &["checkout", branch]) {
                    self.state.handle_error(e);
                    return;
                }
                self.state.notify(
                    NotificationType::Info,
                    format!(
                        "Rebase onto {branch} complete at {}",
                        &new_tip[..new_tip.len().min(8)]
                    ),
                );
                if let Err(e) = self.remove_document_force(doc_id) {
                    self.state.handle_error(e);
                }
                self.refresh_git_status_buffers_for(repo_root);
            }
            RebaseAdvance::PausedForEdit { remaining } => {
                self.render_rebase_pause(
                    doc_id,
                    RebasePause::Edit {
                        remaining: remaining.clone(),
                    },
                    "Paused for edit. Edit files normally, stage with <Space>gs, then ca/cw to amend and resume (or :w here to continue without amending).",
                );
                self.state
                    .notify(NotificationType::Info, "Rebase paused for edit".to_string());
            }
            RebaseAdvance::PausedForReword { message, remaining } => {
                self.render_rebase_pause(
                    doc_id,
                    RebasePause::AwaitingReword {
                        remaining: remaining.clone(),
                    },
                    "Paused for reword — finish the commit message buffer.",
                );
                self.open_git_commit_message(
                    GitCommitTarget::RebaseReword {
                        rebase_doc_id: doc_id,
                    },
                    message,
                );
            }
            RebaseAdvance::Conflict { remaining } => {
                self.render_rebase_pause(
                    doc_id,
                    RebasePause::Conflict {
                        remaining: remaining.clone(),
                    },
                    "Conflict — resolve via <Space>gs, stage the fix, then :w this buffer to continue.",
                );
                self.state.notify(
                    NotificationType::Warning,
                    "Rebase paused: conflict".to_string(),
                );
            }
            RebaseAdvance::Error(e) => {
                self.state.handle_error(e);
            }
        }
    }

    fn render_rebase_pause(&mut self, doc_id: DocumentId, pause: RebasePause, status: &str) {
        let remaining = match &pause {
            RebasePause::Edit { remaining }
            | RebasePause::Conflict { remaining }
            | RebasePause::AwaitingReword { remaining } => remaining.clone(),
        };
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            doc.set_git_rebase_pause(Some(pause));
            doc.render_git_rebase_paused(&remaining, status);
        }
    }

    /// `<Space>gR`/abort key: discard the in-progress rebase and return to the original branch (a no-op detached-HEAD walk, since the branch ref itself is only moved on successful completion).
    pub fn abort_git_rebase(&mut self) {
        let (doc_id, repo_root, branch) = {
            let doc = self.active_document();
            if !doc.is_git_rebase_todo() {
                return;
            }
            let (Some(repo_root), Some(branch)) = (
                doc.git_repo_root().map(Path::to_path_buf),
                doc.git_rebase_branch().map(str::to_string),
            ) else {
                return;
            };
            (doc.id, repo_root, branch)
        };
        let _ = crate::git::run(&repo_root, &["cherry-pick", "--abort"]);
        if let Err(e) = crate::git::run_checked(&repo_root, &["checkout", &branch]) {
            self.state.handle_error(e);
            return;
        }
        self.state
            .notify(NotificationType::Warning, "Rebase aborted".to_string());
        if let Err(e) = self.remove_document_force(doc_id) {
            self.state.handle_error(e);
        }
        self.refresh_git_status_buffers_for(&repo_root);
    }

    /// The commit sha under the cursor in a `GitRebaseTodo` buffer, if any.
    fn git_rebase_cursor_sha(&mut self) -> Option<String> {
        let doc = self.active_document();
        if !doc.is_git_rebase_todo() {
            return None;
        }
        let cursor = doc.buffer.cursor();
        let line = doc.buffer.line_index.get_line_at(cursor);
        doc.annotations.git_rebase_step_at_line(line)
    }

    /// `K`/`J`: move the commit under the cursor up/down one slot.
    pub(super) fn git_rebase_move(&mut self, down: bool) {
        let Some(sha) = self.git_rebase_cursor_sha() else {
            return;
        };
        self.active_document().move_git_rebase_step(&sha, down);
        let _ = self.force_full_redraw();
    }

    /// `p`/`s`/`f`/`e`: set the verb of the commit under the cursor directly, no insert mode. `reword` isn't reachable here; it goes through `c`/`r`'s message editor; `drop` goes through `dd`.
    pub(super) fn git_rebase_set_verb(&mut self, verb: crate::git::rebase::RebaseVerb) {
        let Some(sha) = self.git_rebase_cursor_sha() else {
            return;
        };
        self.active_document().set_git_rebase_verb(&sha, verb);
        let _ = self.force_full_redraw();
    }

    /// `dd`: drop the commit under the cursor from the plan entirely.
    pub(super) fn git_rebase_drop(&mut self) {
        let Some(sha) = self.git_rebase_cursor_sha() else {
            return;
        };
        self.active_document().remove_git_rebase_step(&sha);
        let _ = self.force_full_redraw();
    }

    /// `Enter`/`=`: toggle the commit under the cursor's inline body preview.
    pub(super) fn git_rebase_toggle_fold(&mut self) {
        let Some(sha) = self.git_rebase_cursor_sha() else {
            return;
        };
        let Some(repo_root) = self
            .active_document()
            .git_repo_root()
            .map(Path::to_path_buf)
        else {
            return;
        };
        self.active_document()
            .toggle_git_rebase_expand(&sha, &repo_root);
        let _ = self.force_full_redraw();
    }

    /// `c`/`r`: open the message sub-editor for the commit under the cursor. Planning-time only; nothing's been cherry-picked yet, so saving it just updates the todo's `message_overrides` and returns; it never touches git.
    pub(super) fn git_rebase_open_message_editor(&mut self) {
        let Some(sha) = self.git_rebase_cursor_sha() else {
            return;
        };
        let rebase_doc_id = self.active_document_id();
        let Some(repo_root) = self
            .active_document()
            .git_repo_root()
            .map(Path::to_path_buf)
        else {
            return;
        };
        let initial = self
            .active_document()
            .git_rebase_current_message(&sha, &repo_root);
        self.open_git_commit_message(
            GitCommitTarget::RebasePlanReword { rebase_doc_id, sha },
            initial,
        );
    }

    /// Called when a `RebasePlanReword` message buffer is saved: updates `rebase_doc_id`'s override for `sha`, re-renders it, and returns focus to it; no git command runs for this target.
    pub(super) fn apply_rebase_plan_reword(
        &mut self,
        rebase_doc_id: DocumentId,
        sha: &str,
        message: String,
    ) {
        if let Some(doc) = self.document_manager.get_document_mut(rebase_doc_id) {
            doc.set_git_rebase_message_override(sha, message);
        }
        if let Err(e) = self.document_manager.switch_to_document(rebase_doc_id) {
            self.state.handle_error(e);
            return;
        }
        self.split_tree.set_focused_document(rebase_doc_id);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }
}
