//! Parses and sequences rebase todo steps. The editor layer executes the plan and resumes paused operations.

/// A `Squash` or `Fixup` step folds into the previous surviving step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RebaseVerb {
    Pick,
    Squash,
    Fixup,
    #[cfg(test)]
    Reword,
    Edit,
    #[cfg(test)]
    Drop,
}

impl RebaseVerb {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            #[cfg(test)]
            Self::Reword => "reword",
            Self::Edit => "edit",
            #[cfg(test)]
            Self::Drop => "drop",
        }
    }

    #[cfg(test)]
    /// Parse a verb from its full word or git's single-letter abbreviation
    /// (`p`/`s`/`f`/`r`/`e`/`d`), case-insensitively.
    #[must_use]
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "pick" | "p" => Some(Self::Pick),
            "squash" | "s" => Some(Self::Squash),
            "fixup" | "f" => Some(Self::Fixup),
            "reword" | "r" => Some(Self::Reword),
            "edit" | "e" => Some(Self::Edit),
            "drop" | "d" => Some(Self::Drop),
            _ => None,
        }
    }
}

/// One line of a rebase todo list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RebaseStep {
    pub(crate) verb: RebaseVerb,
    /// Commit sha; full or abbreviated; git's own plumbing (`cherry-pick`, `log -1`) accepts either directly, so no separate resolution step is needed before executing a step.
    pub(crate) sha: String,
    /// Decorative only;  never authoritative; git provides the real message.
    pub(crate) subject: String,
}

/// Why a rebase is currently paused, and what's left to run once it resumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RebasePause {
    /// Paused after cherry-picking an `edit` step's commit. Make changes in
    /// the worktree, then `:w` the todo buffer to resume `remaining`.
    Edit { remaining: Vec<RebaseStep> },
    /// Paused mid-cherry-pick on a conflict. Resolve it (surfaced in the status buffer's Unmerged section), stage the fix, then `:w` to resume: retries `cherry-pick --continue` before running `remaining`.
    Conflict { remaining: Vec<RebaseStep> },
    /// Paused for a `reword` step: a commit-message buffer is open to amend the just-cherry-picked commit. `:w` the todo buffer while in this state is a no-op; finish the message buffer first, which resumes `remaining` itself once saved.
    #[cfg(test)]
    AwaitingReword { remaining: Vec<RebaseStep> },
}

/// Build the initial todo list from `git log --format=<sha>\x1f<subject> base..head`
/// oldest-first output (a rebase replays oldest to newest), all steps `Pick`.
#[must_use]
pub(crate) fn initial_todo_from_log(commits: &[crate::git::log::CommitSummary]) -> Vec<RebaseStep> {
    commits
        .iter()
        .rev() // `git log` prints newest-first; a todo list replays oldest-first.
        .map(|c| RebaseStep {
            verb: RebaseVerb::Pick,
            sha: c.sha.clone(),
            subject: c.subject.clone(),
        })
        .collect()
}

#[cfg(test)]
/// Render a todo list as `<verb> <short-sha> <subject>` lines, git's own
/// `rebase -i` visual format (we parse and execute it ourselves, not git).
#[must_use]
pub(crate) fn render_todo(steps: &[RebaseStep]) -> String {
    steps
        .iter()
        .map(|s| {
            let short = s.sha.get(..8).unwrap_or(s.sha.as_str());
            format!("{} {short} {}", s.verb.as_str(), s.subject)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
/// Parse an edited todo buffer back into steps. Unrecognized lines (bad verb, too few tokens, blank/comment lines) are skipped; a rebase todo, like a git status buffer, tolerates stray text without failing the whole parse. `sha` fields may be abbreviated; resolve with `resolve_short_shas` before executing.
#[must_use]
pub(crate) fn parse_todo(text: &str) -> Vec<RebaseStep> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let mut parts = trimmed.splitn(3, char::is_whitespace);
            let verb = RebaseVerb::parse(parts.next()?)?;
            let sha = parts.next()?.to_string();
            let subject = parts.next().unwrap_or("").trim().to_string();
            Some(RebaseStep { verb, sha, subject })
        })
        .collect()
}

/// Combines squash messages with a blank line and without an editor prompt.
#[must_use]
pub(crate) fn combine_squash_messages(into: &str, incoming: &str) -> String {
    let into = into.trim_end();
    let incoming = incoming.trim();
    if incoming.is_empty() {
        into.to_string()
    } else {
        format!("{into}\n\n{incoming}")
    }
}

#[cfg(test)]
#[path = "rebase_tests.rs"]
mod rebase_tests;
