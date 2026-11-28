use super::Editor;
use crate::document::DocumentId;
use crate::git::query::{GitOutput, GitQuery, GitView};
use crate::job_manager::jobs::git::{GitJob, GitResult, GitSink};
use crate::job_manager::{AsyncToken, JobMessage};
use crate::term::TerminalBackend;
use std::path::PathBuf;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn open_git(&mut self, query: GitQuery) {
        let repo_root = match self.resolve_git_repo_root() {
            Ok(root) => root,
            Err(e) => {
                self.notify_error(e);
                return;
            }
        };
        let view = query.view();
        let query = if view == GitView::Blame && query.blame_target().path.is_none() {
            let Some(path) = self.resolve_git_blame_target() else {
                return;
            };
            let mut args = query.args().to_vec();
            args.push("--".to_string());
            args.push(path.to_string_lossy().into_owned());
            GitQuery::new(args)
        } else {
            query
        };
        self.job_manager
            .spawn(GitJob::new(repo_root, query, view, GitSink::Open));
    }

    pub(super) fn refresh_git_view(&mut self, doc_id: DocumentId) -> Option<usize> {
        let doc = self.document_manager.get_document(doc_id)?;
        let repo_root = doc.git_repo_root()?.to_path_buf();
        let (query, view) = doc.git_view_query()?;
        Some(self.job_manager.spawn(GitJob::new(
            repo_root,
            query,
            view,
            GitSink::Refresh(doc_id),
        )))
    }

    pub(super) fn fetch_git_status_hunks(
        &mut self,
        doc: DocumentId,
        repo_root: PathBuf,
        path: PathBuf,
        staged_side: bool,
        untracked: bool,
    ) {
        let path_arg = path.to_string_lossy().into_owned();
        let query = if untracked {
            GitQuery::new([
                "diff",
                "--no-index",
                "-U3",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--",
                "/dev/null",
                &path_arg,
            ])
        } else {
            let mut args = vec!["diff", "-U3"];
            if staged_side {
                args.push("--cached");
            }
            args.extend(["--", &path_arg]);
            GitQuery::new(args)
        };
        self.job_manager.spawn(GitJob::new(
            repo_root,
            query,
            GitView::Diff,
            GitSink::StatusHunks {
                doc,
                path,
                staged_side,
            },
        ));
    }

    pub(super) fn fetch_git_commit_body(
        &mut self,
        doc: DocumentId,
        repo_root: PathBuf,
        sha: String,
    ) {
        self.job_manager.spawn(GitJob::new(
            repo_root,
            GitQuery::new(["show", sha.as_str()]),
            GitView::Text,
            GitSink::CommitBody { doc, sha },
        ));
    }

    fn expand_first_git_commit(&mut self, doc_id: DocumentId) {
        let Some((repo_root, sha)) = self.document_manager.get_document(doc_id).and_then(|doc| {
            let repo_root = doc.git_repo_root()?.to_path_buf();
            let sha = doc.git_log_commits()?.first()?.sha.clone();
            Some((repo_root, sha))
        }) else {
            return;
        };
        self.fetch_git_commit_body(doc_id, repo_root, sha);
    }

    pub(super) fn finish_git_job(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: GitResult,
    ) {
        let GitResult {
            repo_root,
            query,
            view,
            sink,
            output,
        } = result;
        match sink.doc().zip(sink.domain(view)) {
            None => self.open_git_view(repo_root, query, view, output),
            Some((doc_id, domain)) => {
                if let Some(token) = self.resolve_and_validate_token(id, token, doc_id, domain) {
                    self.update_git_view(token, &repo_root, view, sink, output);
                }
            }
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn open_git_view(
        &mut self,
        repo_root: PathBuf,
        query: GitQuery,
        view: GitView,
        output: GitOutput,
    ) {
        let id = self.document_manager.next_id();
        let doc = match output {
            GitOutput::Status {
                snapshot,
                head_subject,
            } => crate::document::Document::new_git_status(id, repo_root, query).map(|mut doc| {
                doc.populate_git_status_buffer(snapshot, head_subject);
                doc
            }),
            GitOutput::Log(commits) => crate::document::Document::new_git_log(id, repo_root, query)
                .map(|mut doc| {
                    doc.populate_git_log_buffer(commits);
                    doc
                }),
            GitOutput::Blame(lines) => {
                let target = query.blame_target();
                if let Some(path) = target.path {
                    self.open_git_blame(
                        &repo_root.join(path),
                        &repo_root,
                        target.flags,
                        target.rev.as_deref(),
                        lines,
                    );
                }
                return;
            }
            GitOutput::Diff(_) => return,
            GitOutput::Text { text, success } => {
                self.refresh_git_status_buffers_for(&repo_root);
                let trimmed = text.trim();
                if trimmed.lines().count() > 1 || trimmed.len() > 200 {
                    Ok(crate::document::Document::new_git_output(
                        id, repo_root, query, &text,
                    ))
                } else {
                    let (kind, message) = if trimmed.is_empty() {
                        (
                            crate::notification::NotificationType::Info,
                            format!("git {}: done", query.label()),
                        )
                    } else if success {
                        (
                            crate::notification::NotificationType::Info,
                            trimmed.to_string(),
                        )
                    } else {
                        (
                            crate::notification::NotificationType::Error,
                            trimmed.to_string(),
                        )
                    };
                    self.notify(kind, message);
                    return;
                }
            }
        };
        let doc = match doc {
            Ok(doc) => doc,
            Err(e) => {
                self.notify_error(e);
                return;
            }
        };
        self.open_buffer(doc, crate::document::manager::Placement::Background);
        if let Err(e) = self.focus_document(id) {
            self.notify_error(e);
            return;
        }
        if view == GitView::Show {
            self.expand_first_git_commit(id);
        }
    }

    fn update_git_view(
        &mut self,
        token: AsyncToken,
        repo_root: &std::path::Path,
        view: GitView,
        sink: GitSink,
        output: GitOutput,
    ) {
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            return;
        };
        if doc.git_repo_root() != Some(repo_root) {
            return;
        }
        let doc_id = doc.id;
        match (sink, output) {
            (
                GitSink::Refresh(_),
                GitOutput::Status {
                    snapshot,
                    head_subject,
                },
            ) if doc.is_git_status() => doc.populate_git_status_buffer(snapshot, head_subject),
            (GitSink::Refresh(_), GitOutput::Log(commits)) if doc.is_git_log() => {
                doc.populate_git_log_buffer(commits);
                if view == GitView::Show {
                    self.expand_first_git_commit(doc_id);
                }
            }
            (GitSink::Refresh(_), GitOutput::Blame(lines)) if doc.is_git_blame() => {
                doc.populate_git_blame_buffer(lines);
            }
            (
                GitSink::StatusHunks {
                    path, staged_side, ..
                },
                GitOutput::Diff(hunks),
            ) if doc.is_git_status() => doc.set_git_status_expanded(path, staged_side, hunks),
            (
                GitSink::CommitBody { sha, .. },
                GitOutput::Text {
                    text,
                    success: true,
                },
            ) if doc.is_git_log() => {
                doc.set_git_log_expanded(Some(sha), Some(text));
            }
            (GitSink::Refresh(_), GitOutput::Text { text, .. }) if doc.is_git_output() => {
                doc.populate_git_output(&text);
            }
            (_, GitOutput::Text { text, .. }) => self.notify(
                crate::notification::NotificationType::Error,
                text.trim().to_string(),
            ),
            _ => {}
        }
    }
}
