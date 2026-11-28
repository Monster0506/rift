use super::blame::BlameLine;
use super::diff::Hunk;
use super::log::CommitSummary;
use super::status::StatusSnapshot;
use crate::error::RiftError;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitQuery {
    args: Vec<String>,
}

impl GitQuery {
    pub(crate) fn new<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub(crate) fn parse(line: &str) -> Self {
        let query = Self::new(line.split_whitespace());
        if query.args.is_empty() {
            Self::new(["status"])
        } else {
            query
        }
    }

    pub(crate) fn args(&self) -> &[String] {
        &self.args
    }

    pub(crate) fn label(&self) -> String {
        self.args.join(" ")
    }

    pub(crate) fn view(&self) -> GitView {
        match self.args.first().map(String::as_str) {
            Some("status") => GitView::Status,
            Some("log") => GitView::Log,
            Some("show") => GitView::Show,
            Some("blame") => GitView::Blame,
            _ => GitView::Text,
        }
    }

    pub(crate) fn blame_target(&self) -> BlameTarget {
        let rest = self.args.get(1..).unwrap_or_default();
        let (before, path) = match rest.iter().position(|arg| arg == "--") {
            Some(split) => (&rest[..split], rest[split + 1..].last().cloned()),
            None => match rest.iter().rposition(|arg| !arg.starts_with('-')) {
                Some(last) => (&rest[..last], Some(rest[last].clone())),
                None => (rest, None),
            },
        };
        BlameTarget {
            flags: before
                .iter()
                .filter(|arg| arg.starts_with('-'))
                .cloned()
                .collect(),
            rev: before.iter().find(|arg| !arg.starts_with('-')).cloned(),
            path,
        }
    }

    fn with_flags(&self, flags: &[String]) -> Vec<String> {
        let mut args = Vec::with_capacity(self.args.len() + flags.len());
        args.extend(self.args.first().cloned());
        args.extend_from_slice(flags);
        args.extend(self.args.iter().skip(1).cloned());
        args
    }
}

pub(crate) struct BlameTarget {
    pub(crate) flags: Vec<String>,
    pub(crate) rev: Option<String>,
    pub(crate) path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitView {
    Status,
    Log,
    Show,
    Blame,
    Diff,
    Text,
}

impl GitView {
    fn flags(self) -> Option<Vec<String>> {
        let log_format = || format!("--format={}", super::log::LOG_FORMAT);
        Some(match self {
            Self::Status => vec![
                "--porcelain=v2".into(),
                "--branch".into(),
                "--untracked-files=all".into(),
            ],
            Self::Log => vec![log_format()],
            Self::Show => vec!["--no-patch".into(), log_format()],
            Self::Blame => vec!["--porcelain".into()],
            Self::Diff => vec!["--no-ext-diff".into()],
            Self::Text => return None,
        })
    }

    fn parse(self, repo_root: &Path, stdout: &str) -> Option<GitOutput> {
        match self {
            Self::Status => {
                let snapshot = super::status::parse_status_exact(stdout)?;
                let head_subject = super::run_checked(repo_root, &["log", "-1", "--format=%s"])
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                Some(GitOutput::Status {
                    snapshot,
                    head_subject,
                })
            }
            Self::Log | Self::Show => super::log::parse_log_exact(stdout).map(GitOutput::Log),
            Self::Blame => super::blame::parse_blame_exact(stdout).map(GitOutput::Blame),
            Self::Diff => super::diff::parse_unified_diff_exact(stdout).map(|files| {
                GitOutput::Diff(files.into_iter().next().map_or_default(|file| file.hunks))
            }),
            Self::Text => None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum GitOutput {
    Status {
        snapshot: StatusSnapshot,
        head_subject: Option<String>,
    },
    Log(Vec<CommitSummary>),
    Blame(Vec<BlameLine>),
    Diff(Vec<Hunk>),
    Text {
        text: String,
        success: bool,
    },
}

/// # Errors
/// Returns `Err` when `git` cannot be spawned.
pub(crate) fn run(
    repo_root: &Path,
    query: &GitQuery,
    view: GitView,
) -> Result<GitOutput, RiftError> {
    if let Some(flags) = view.flags() {
        let args = query.with_flags(&flags);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = super::run(repo_root, &args)?;
        let failed = !out.success && out.stdout.trim().is_empty();
        if !failed {
            if let Some(output) = view.parse(repo_root, &out.stdout) {
                return Ok(output);
            }
        }
    }
    let args: Vec<&str> = query.args.iter().map(String::as_str).collect();
    let out = super::run(repo_root, &args)?;
    let mut text = out.stdout;
    if !out.stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&out.stderr);
    }
    Ok(GitOutput::Text {
        text,
        success: out.success,
    })
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod query_tests;
