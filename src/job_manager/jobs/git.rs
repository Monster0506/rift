//! Background jobs for git status/diff, following the `DirectoryListJob` template (`job_manager::jobs::explorer`): construct with a `doc_id` cast to `usize`, `run()` shells out and sends a `Custom` payload back.

use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// Result of a `GitStatusJob`: a freshly parsed status snapshot for `doc_id`.
#[derive(Debug)]
pub struct GitStatusResult {
    pub doc_id: usize,
    pub repo_root: PathBuf,
    pub snapshot: crate::git::status::StatusSnapshot,
    /// HEAD's subject line, for the status header's `HEAD <sha> <subject>` summary (Enter on it opens the Log browser). `None` on an unborn branch with no commits yet.
    pub head_subject: Option<String>,
}
crate::impl_job_payload!(GitStatusResult);

/// Runs `git status --porcelain=v2 --branch --untracked-files=all` and parses it.
#[derive(Debug)]
pub struct GitStatusJob {
    doc_id: usize,
    repo_root: PathBuf,
    token: Option<crate::job_manager::AsyncToken>,
}

impl GitStatusJob {
    pub fn new(doc_id: usize, repo_root: PathBuf) -> Self {
        Self {
            doc_id,
            repo_root,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }
}

impl Job for GitStatusJob {
    fn name(&self) -> &'static str {
        "git-status"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<crate::document::DocumentId> {
        Some(self.doc_id as crate::document::DocumentId)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::GitStatus)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        match crate::git::run_checked(
            &self.repo_root,
            &[
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=all",
            ],
        ) {
            Ok(stdout) => {
                let snapshot = crate::git::status::parse_status(&stdout);
                let head_subject =
                    crate::git::run_checked(&self.repo_root, &["log", "-1", "--format=%s"])
                        .ok()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                let result = Box::new(GitStatusResult {
                    doc_id: self.doc_id,
                    repo_root: self.repo_root,
                    snapshot,
                    head_subject,
                });
                if let Some(token) = self.token {
                    crate::job_manager::send_job_result_with_token(&sender, id, token, result);
                } else {
                    crate::job_manager::send_job_result(&sender, id, result);
                }
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.message));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Result of a `GitDiffJob`: hunks for one file's diff, to expand inline
/// under its entry in a status buffer.
#[derive(Debug)]
pub struct GitDiffResult {
    pub doc_id: usize,
    pub path: PathBuf,
    /// `true` = this came from `git diff --cached` (the staged side).
    pub staged_side: bool,
    pub hunks: Vec<crate::git::diff::Hunk>,
}
crate::impl_job_payload!(GitDiffResult);

/// Runs `git diff --no-ext-diff [--cached] -- <path>` scoped to one file and
/// parses it, for status-buffer hunk expansion (`=`).
#[derive(Debug)]
pub struct GitDiffJob {
    doc_id: usize,
    repo_root: PathBuf,
    path: PathBuf,
    staged_side: bool,
    /// Untracked (no index entry at all yet): fetched via `git diff --no-index` against `/dev/null` instead of `git diff [--cached]`, since a plain `git diff` never shows untracked paths.
    untracked: bool,
    token: Option<crate::job_manager::AsyncToken>,
}

impl GitDiffJob {
    pub fn new(
        doc_id: usize,
        repo_root: PathBuf,
        path: PathBuf,
        staged_side: bool,
        untracked: bool,
    ) -> Self {
        Self {
            doc_id,
            repo_root,
            path,
            staged_side,
            untracked,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }
}

impl Job for GitDiffJob {
    fn name(&self) -> &'static str {
        "git-diff"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<crate::document::DocumentId> {
        Some(self.doc_id as crate::document::DocumentId)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::GitDiff)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        let path_str = self.path.to_string_lossy().into_owned();
        let stdout = if self.untracked {
            // A plain `git diff` never shows untracked paths at all; diff against `/dev/null` directly. `--src-prefix`/`--dst-prefix` force the conventional `a/`/`b/` headers `parse_unified_diff` and `git apply --cached` (staging) both expect; `--no-index` otherwise numbers them `1/`/`2/` outside a real repo pair.
            let args = [
                "diff",
                "--no-ext-diff",
                "--no-index",
                "-U3",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--",
                "/dev/null",
                &path_str,
            ];
            // `--no-index` exits 1 (not a failure) whenever there IS a difference; always true here, since the file exists and `/dev/null` doesn't; so read stdout regardless of `success`.
            match crate::git::run(&self.repo_root, &args) {
                Ok(out) => out.stdout,
                Err(e) => {
                    let _ = sender.send(JobMessage::Error(id, e.message));
                    return;
                }
            }
        } else {
            let mut args = vec!["diff", "--no-ext-diff", "-U3"];
            if self.staged_side {
                args.push("--cached");
            }
            args.push("--");
            args.push(&path_str);
            match crate::git::run_checked(&self.repo_root, &args) {
                Ok(stdout) => stdout,
                Err(e) => {
                    let _ = sender.send(JobMessage::Error(id, e.message));
                    return;
                }
            }
        };

        let files = crate::git::diff::parse_unified_diff(&stdout);
        let hunks = files
            .into_iter()
            .next()
            .map(|f| f.hunks)
            .unwrap_or_default();
        let result = Box::new(GitDiffResult {
            doc_id: self.doc_id,
            path: self.path,
            staged_side: self.staged_side,
            hunks,
        });
        if let Some(token) = self.token {
            crate::job_manager::send_job_result_with_token(&sender, id, token, result);
        } else {
            crate::job_manager::send_job_result(&sender, id, result);
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Result of a `GitBlameJob`: a freshly parsed blame listing for `doc_id`.
#[derive(Debug)]
pub struct GitBlameResult {
    pub doc_id: usize,
    pub lines: Vec<crate::git::blame::BlameLine>,
}
crate::impl_job_payload!(GitBlameResult);

/// Runs `git blame --porcelain [<commit>] -- <path>` and parses it.
#[derive(Debug)]
pub struct GitBlameJob {
    doc_id: usize,
    repo_root: PathBuf,
    path: PathBuf,
    at_commit: Option<String>,
    token: Option<crate::job_manager::AsyncToken>,
}

impl GitBlameJob {
    pub fn new(
        doc_id: usize,
        repo_root: PathBuf,
        path: PathBuf,
        at_commit: Option<String>,
    ) -> Self {
        Self {
            doc_id,
            repo_root,
            path,
            at_commit,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }
}

impl Job for GitBlameJob {
    fn name(&self) -> &'static str {
        "git-blame"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<crate::document::DocumentId> {
        Some(self.doc_id as crate::document::DocumentId)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::GitBlame)
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        let path_str = self.path.to_string_lossy().into_owned();
        let mut args = vec!["blame", "--porcelain"];
        if let Some(commit) = &self.at_commit {
            args.push(commit);
        }
        args.push("--");
        args.push(&path_str);

        match crate::git::run_checked(&self.repo_root, &args) {
            Ok(stdout) => {
                let lines = crate::git::blame::parse_blame(&stdout);
                let result = Box::new(GitBlameResult {
                    doc_id: self.doc_id,
                    lines,
                });
                if let Some(token) = self.token {
                    crate::job_manager::send_job_result_with_token(&sender, id, token, result);
                } else {
                    crate::job_manager::send_job_result(&sender, id, result);
                }
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.message));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Result of a `GitLogJob`: a freshly parsed commit listing for `doc_id`.
#[derive(Debug)]
pub struct GitLogResult {
    pub doc_id: usize,
    pub commits: Vec<crate::git::log::CommitSummary>,
}
crate::impl_job_payload!(GitLogResult);

/// Runs `git log --format=<LOG_FORMAT> [-- <path>]` and parses it.
#[derive(Debug)]
pub struct GitLogJob {
    doc_id: usize,
    repo_root: PathBuf,
    path: Option<PathBuf>,
    token: Option<crate::job_manager::AsyncToken>,
}

impl GitLogJob {
    pub fn new(doc_id: usize, repo_root: PathBuf, path: Option<PathBuf>) -> Self {
        Self {
            doc_id,
            repo_root,
            path,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }
}

impl Job for GitLogJob {
    fn name(&self) -> &'static str {
        "git-log"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<crate::document::DocumentId> {
        Some(self.doc_id as crate::document::DocumentId)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::GitLog)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        let format_arg = format!("--format={}", crate::git::log::LOG_FORMAT);
        let mut args = vec!["log", &format_arg];
        let path_str = self.path.as_ref().map(|p| p.to_string_lossy().into_owned());
        if let Some(p) = &path_str {
            args.push("--");
            args.push(p);
        }

        match crate::git::run_checked(&self.repo_root, &args) {
            Ok(stdout) => {
                let commits = crate::git::log::parse_log(&stdout);
                let result = Box::new(GitLogResult {
                    doc_id: self.doc_id,
                    commits,
                });
                if let Some(token) = self.token {
                    crate::job_manager::send_job_result_with_token(&sender, id, token, result);
                } else {
                    crate::job_manager::send_job_result(&sender, id, result);
                }
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.message));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Result of a `GitShowJob`: the raw `git show` body for one commit, to
/// expand inline under it in a log buffer. Opaque text, not reparsed.
#[derive(Debug)]
pub struct GitShowResult {
    pub doc_id: usize,
    pub sha: String,
    pub body: String,
}
crate::impl_job_payload!(GitShowResult);

/// Runs `git show <sha>` for view-only output and honors a configured `diff.external`.
#[derive(Debug)]
pub struct GitShowJob {
    doc_id: usize,
    repo_root: PathBuf,
    sha: String,
    token: Option<crate::job_manager::AsyncToken>,
}

impl GitShowJob {
    pub fn new(doc_id: usize, repo_root: PathBuf, sha: String) -> Self {
        Self {
            doc_id,
            repo_root,
            sha,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }
}

impl Job for GitShowJob {
    fn name(&self) -> &'static str {
        "git-show"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<crate::document::DocumentId> {
        Some(self.doc_id as crate::document::DocumentId)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::GitShow)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        match crate::git::run_checked(&self.repo_root, &["show", &self.sha]) {
            Ok(stdout) => {
                let result = Box::new(GitShowResult {
                    doc_id: self.doc_id,
                    sha: self.sha,
                    body: stdout,
                });
                if let Some(token) = self.token {
                    crate::job_manager::send_job_result_with_token(&sender, id, token, result);
                } else {
                    crate::job_manager::send_job_result(&sender, id, result);
                }
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.message));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Result of a `GitCommandJob`: combined stdout+stderr for an arbitrary
/// `git <args>` invocation (`:Git <args>` escape hatch).
#[derive(Debug)]
pub struct GitCommandResult {
    /// The active document at the time `:Git` was run, so the editor can return focus there after showing output (not otherwise used for staleness checks; this job is one-shot, not tied to a specific buffer).
    pub origin_doc_id: usize,
    pub repo_root: PathBuf,
    pub args: String,
    pub output: String,
    pub success: bool,
}
crate::impl_job_payload!(GitCommandResult);

/// Runs `git <args>` with whitespace splitting and captures stdout plus stderr for display.
#[derive(Debug)]
pub struct GitCommandJob {
    origin_doc_id: usize,
    repo_root: PathBuf,
    args: String,
}

impl GitCommandJob {
    pub fn new(origin_doc_id: usize, repo_root: PathBuf, args: String) -> Self {
        Self {
            origin_doc_id,
            repo_root,
            args,
        }
    }
}

impl Job for GitCommandJob {
    fn name(&self) -> &'static str {
        "git-command"
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        let arg_tokens: Vec<&str> = self.args.split_whitespace().collect();
        let output = crate::git::run(&self.repo_root, &arg_tokens);
        let (text, success) = match output {
            Ok(out) => {
                let mut combined = out.stdout;
                if !out.stderr.is_empty() {
                    if !combined.is_empty() && !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                    combined.push_str(&out.stderr);
                }
                (combined, out.success)
            }
            Err(e) => (e.message, false),
        };
        let result = Box::new(GitCommandResult {
            origin_doc_id: self.origin_doc_id,
            repo_root: self.repo_root,
            args: self.args,
            output: text,
            success,
        });
        crate::job_manager::send_job_result(&sender, id, result);
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Result of a `GitGutterDiffJob`: fresh gutter signs for `doc_id`, tagged with the buffer revision they were computed from so a stale result (the buffer changed again while this job was running) can be discarded.
#[derive(Debug)]
pub struct GitGutterDiffResult {
    pub doc_id: usize,
    pub revision: u64,
    pub signs: Vec<(usize, crate::git::diff::GutterSignKind)>,
}
crate::impl_job_payload!(GitGutterDiffResult);

/// Diffs a `File` buffer's live (possibly unsaved) content against its git index version, for gutter-diff signs. Writes both sides to temp files and shells `git diff --no-ext-diff --no-index -U0`, since there's no `git` plumbing that diffs an index blob against arbitrary in-memory text directly..
#[derive(Debug)]
pub struct GitGutterDiffJob {
    doc_id: usize,
    revision: u64,
    repo_root: PathBuf,
    rel_path: PathBuf,
    buffer_text: String,
    token: Option<crate::job_manager::AsyncToken>,
}

impl GitGutterDiffJob {
    pub fn new(
        doc_id: usize,
        revision: u64,
        repo_root: PathBuf,
        rel_path: PathBuf,
        buffer_text: String,
    ) -> Self {
        Self {
            doc_id,
            revision,
            repo_root,
            rel_path,
            buffer_text,
            token: None,
        }
    }

    pub fn with_token(mut self, token: crate::job_manager::AsyncToken) -> Self {
        self.token = Some(token);
        self
    }
}

impl Job for GitGutterDiffJob {
    fn name(&self) -> &'static str {
        "git-gutter-diff"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<crate::document::DocumentId> {
        Some(self.doc_id as crate::document::DocumentId)
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        Some(crate::job_manager::AsyncOpDomain::GitGutter)
    }
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        // Git's `:path` index syntax always wants '/' regardless of host OS (the index stores paths that way internally); a Windows backslash path here silently misses the index entry, since this isn't a general filesystem path lookup that Windows normalizes.
        let rel_path_str = self.rel_path.to_string_lossy().replace('\\', "/");
        // ":<path>" reads the index (staged) version; this is what "unsaved
        // changes vs. what you last staged/committed" should compare against.
        let index_arg = format!(":{rel_path_str}");
        let index_content = match crate::git::run(&self.repo_root, &["show", &index_arg]) {
            Ok(out) if out.success => out.stdout,
            _ => {
                // Not in the index (untracked, or newly created);  no baseline, no signs.
                let result = Box::new(GitGutterDiffResult {
                    doc_id: self.doc_id,
                    revision: self.revision,
                    signs: Vec::new(),
                });
                if let Some(token) = self.token {
                    crate::job_manager::send_job_result_with_token(&sender, id, token, result);
                } else {
                    crate::job_manager::send_job_result(&sender, id, result);
                }
                return;
            }
        };

        if signal.is_cancelled() {
            return;
        }

        let index_tmp = match tempfile_write(&index_content) {
            Ok(f) => f,
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e));
                return;
            }
        };
        let buffer_tmp = match tempfile_write(&self.buffer_text) {
            Ok(f) => f,
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e));
                return;
            }
        };

        let index_path = index_tmp.to_string_lossy().into_owned();
        let buffer_path = buffer_tmp.to_string_lossy().into_owned();
        let diff_out = crate::git::run(
            &self.repo_root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-index",
                "-U0",
                &index_path,
                &buffer_path,
            ],
        );
        let _ = std::fs::remove_file(&index_tmp);
        let _ = std::fs::remove_file(&buffer_tmp);

        // `git diff --no-index` exits 1 (not a failure) when there IS a
        // difference, so read stdout regardless of `success`.
        let stdout = match diff_out {
            Ok(out) => out.stdout,
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e.message));
                return;
            }
        };
        let files = crate::git::diff::parse_unified_diff(&stdout);
        let signs = files
            .first()
            .map(|f| crate::git::diff::classify_gutter_signs(&f.hunks))
            .unwrap_or_default();

        let result = Box::new(GitGutterDiffResult {
            doc_id: self.doc_id,
            revision: self.revision,
            signs,
        });
        if let Some(token) = self.token {
            crate::job_manager::send_job_result_with_token(&sender, id, token, result);
        } else {
            crate::job_manager::send_job_result(&sender, id, result);
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Write `content` to a fresh temp file, returning its path. Named oddly-but-clearly `tempfile_write` to avoid confusion with the `tempfile` dev-dependency crate (unavailable in non-test builds; this is hand-rolled).
fn tempfile_write(content: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir();
    let unique = format!(
        "rift-gutter-diff-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let path = dir.join(unique);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}
