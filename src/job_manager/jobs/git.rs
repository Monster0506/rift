use crate::document::DocumentId;
use crate::git::query::{GitOutput, GitQuery, GitView};
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload, JobRef, JobTarget};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub(crate) enum GitSink {
    Open,
    Refresh(DocumentId),
    StatusHunks {
        doc: DocumentId,
        path: PathBuf,
        staged_side: bool,
    },
    CommitBody {
        doc: DocumentId,
        sha: String,
    },
}

impl GitSink {
    pub(crate) const fn doc(&self) -> Option<DocumentId> {
        match self {
            Self::Open => None,
            Self::Refresh(doc) | Self::StatusHunks { doc, .. } | Self::CommitBody { doc, .. } => {
                Some(*doc)
            }
        }
    }

    pub(crate) const fn domain(&self, view: GitView) -> Option<crate::job_manager::AsyncOpDomain> {
        use crate::job_manager::AsyncOpDomain;
        match self {
            Self::Open => None,
            Self::Refresh(_) => match view {
                GitView::Status => Some(AsyncOpDomain::GitStatus),
                GitView::Log | GitView::Show => Some(AsyncOpDomain::GitLog),
                GitView::Blame => Some(AsyncOpDomain::GitBlame),
                GitView::Text => Some(AsyncOpDomain::GitOutput),
                GitView::Diff => None,
            },
            Self::StatusHunks { .. } => Some(AsyncOpDomain::GitDiff),
            Self::CommitBody { .. } => Some(AsyncOpDomain::GitShow),
        }
    }
}

#[derive(Debug)]
pub struct GitResult {
    pub(crate) repo_root: PathBuf,
    pub(crate) query: GitQuery,
    pub(crate) view: GitView,
    pub(crate) sink: GitSink,
    pub(crate) output: GitOutput,
}
impl JobPayload for GitResult {
    fn apply(self: Box<Self>, editor: &mut dyn JobTarget, job: JobRef) {
        editor.apply_git(job.id, job.token, *self);
    }
}

#[derive(Debug)]
pub(crate) struct GitJob {
    repo_root: PathBuf,
    query: GitQuery,
    view: GitView,
    sink: GitSink,
}

impl GitJob {
    #[must_use]
    pub(crate) const fn new(
        repo_root: PathBuf,
        query: GitQuery,
        view: GitView,
        sink: GitSink,
    ) -> Self {
        Self {
            repo_root,
            query,
            view,
            sink,
        }
    }
}

impl Job for GitJob {
    fn name(&self) -> &'static str {
        "git"
    }

    fn target_document_id(&self) -> Option<DocumentId> {
        self.sink.doc()
    }

    fn target_domain(&self) -> Option<crate::job_manager::AsyncOpDomain> {
        self.sink.domain(self.view)
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        match crate::git::query::run(&self.repo_root, &self.query, self.view) {
            Ok(output) => {
                let result = Box::new(GitResult {
                    repo_root: self.repo_root,
                    query: self.query,
                    view: self.view,
                    sink: self.sink,
                    output,
                });
                crate::job_manager::send_job_result(&sender, id, result);
            }
            Err(e) => {
                let _ = sender.send(JobMessage::Error(id, e));
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}

/// Result of a `GitGutterDiffJob`: fresh gutter signs for `doc_id`, tagged with the buffer revision they were computed from so a stale result (the buffer changed again while this job was running) can be discarded.
#[derive(Debug)]
pub struct GitGutterDiffResult {
    pub(crate) doc_id: DocumentId,
    pub(crate) revision: u64,
    pub(crate) signs: Vec<(usize, crate::git::diff::GutterSignKind)>,
}
impl JobPayload for GitGutterDiffResult {
    fn apply(self: Box<Self>, editor: &mut dyn JobTarget, job: JobRef) {
        editor.apply_git_gutter(job.id, job.token, *self);
    }
}

/// Diffs a `File` buffer's live (possibly unsaved) content against its git index version, for gutter-diff signs. Writes both sides to temp files and shells `git diff --no-ext-diff --no-index -U0`, since there's no `git` plumbing that diffs an index blob against arbitrary in-memory text directly..
#[derive(Debug)]
pub(crate) struct GitGutterDiffJob {
    doc_id: DocumentId,
    revision: u64,
    repo_root: PathBuf,
    rel_path: PathBuf,
    buffer_text: String,
    token: Option<crate::job_manager::AsyncToken>,
}

impl GitGutterDiffJob {
    #[must_use]
    pub(crate) const fn new(
        doc_id: DocumentId,
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
}

impl Job for GitGutterDiffJob {
    fn name(&self) -> &'static str {
        "git-gutter-diff"
    }

    fn async_token(&self) -> Option<crate::job_manager::AsyncToken> {
        self.token
    }

    fn target_document_id(&self) -> Option<crate::document::DocumentId> {
        Some(self.doc_id)
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
                let _ = sender.send(JobMessage::Error(id, e));
                return;
            }
        };
        let files = crate::git::diff::parse_unified_diff(&stdout);
        let signs = files
            .first()
            .map_or_default(|f| crate::git::diff::classify_gutter_signs(&f.hunks));

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
fn tempfile_write(content: &str) -> Result<PathBuf, crate::error::RiftError> {
    let dir = std::env::temp_dir();
    let unique = format!(
        "rift-gutter-diff-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    let path = dir.join(unique);
    std::fs::write(&path, content)?;
    Ok(path)
}
