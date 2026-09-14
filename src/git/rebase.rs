//! Parses and sequences rebase todo steps. The editor layer executes the plan and resumes paused operations.

/// A `Squash` or `Fixup` step folds into the previous surviving step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseVerb {
    Pick,
    Squash,
    Fixup,
    Reword,
    Edit,
    Drop,
}

impl RebaseVerb {
    pub fn as_str(self) -> &'static str {
        match self {
            RebaseVerb::Pick => "pick",
            RebaseVerb::Squash => "squash",
            RebaseVerb::Fixup => "fixup",
            RebaseVerb::Reword => "reword",
            RebaseVerb::Edit => "edit",
            RebaseVerb::Drop => "drop",
        }
    }

    /// Parse a verb from its full word or git's single-letter abbreviation
    /// (`p`/`s`/`f`/`r`/`e`/`d`), case-insensitively.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "pick" | "p" => Some(RebaseVerb::Pick),
            "squash" | "s" => Some(RebaseVerb::Squash),
            "fixup" | "f" => Some(RebaseVerb::Fixup),
            "reword" | "r" => Some(RebaseVerb::Reword),
            "edit" | "e" => Some(RebaseVerb::Edit),
            "drop" | "d" => Some(RebaseVerb::Drop),
            _ => None,
        }
    }
}

/// One line of a rebase todo list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseStep {
    pub verb: RebaseVerb,
    /// Commit sha; full or abbreviated; git's own plumbing (`cherry-pick`, `log -1`) accepts either directly, so no separate resolution step is needed before executing a step.
    pub sha: String,
    /// Decorative only;  never authoritative; git provides the real message.
    pub subject: String,
}

/// Why a rebase is currently paused, and what's left to run once it resumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebasePause {
    /// Paused after cherry-picking an `edit` step's commit. Make changes in
    /// the worktree, then `:w` the todo buffer to resume `remaining`.
    Edit { remaining: Vec<RebaseStep> },
    /// Paused mid-cherry-pick on a conflict. Resolve it (surfaced in the status buffer's Unmerged section), stage the fix, then `:w` to resume: retries `cherry-pick --continue` before running `remaining`.
    Conflict { remaining: Vec<RebaseStep> },
    /// Paused for a `reword` step: a commit-message buffer is open to amend the just-cherry-picked commit. `:w` the todo buffer while in this state is a no-op; finish the message buffer first, which resumes `remaining` itself once saved.
    AwaitingReword { remaining: Vec<RebaseStep> },
}

/// Build the initial todo list from `git log --format=<sha>\x1f<subject> base..head`
/// oldest-first output (a rebase replays oldest to newest), all steps `Pick`.
pub fn initial_todo_from_log(commits: &[crate::git::log::CommitSummary]) -> Vec<RebaseStep> {
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

/// Render a todo list as `<verb> <short-sha> <subject>` lines, git's own
/// `rebase -i` visual format (we parse and execute it ourselves, not git).
pub fn render_todo(steps: &[RebaseStep]) -> String {
    steps
        .iter()
        .map(|s| {
            let short = &s.sha[..s.sha.len().min(8)];
            format!("{} {short} {}", s.verb.as_str(), s.subject)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse an edited todo buffer back into steps. Unrecognized lines (bad verb, too few tokens, blank/comment lines) are skipped; a rebase todo, like a git status buffer, tolerates stray text without failing the whole parse. `sha` fields may be abbreviated; resolve with `resolve_short_shas` before executing.
pub fn parse_todo(text: &str) -> Vec<RebaseStep> {
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
pub fn combine_squash_messages(into: &str, incoming: &str) -> String {
    let into = into.trim_end();
    let incoming = incoming.trim();
    if incoming.is_empty() {
        into.to_string()
    } else {
        format!("{into}\n\n{incoming}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(sha: &str, subject: &str) -> crate::git::log::CommitSummary {
        crate::git::log::CommitSummary {
            sha: sha.to_string(),
            short_sha: sha[..sha.len().min(8)].to_string(),
            author_name: "A".to_string(),
            author_email: "a@example.com".to_string(),
            author_time: 0,
            subject: subject.to_string(),
            body: String::new(),
            parents: Vec::new(),
        }
    }

    #[test]
    fn verb_parses_full_words_and_abbreviations_case_insensitively() {
        assert_eq!(RebaseVerb::parse("pick"), Some(RebaseVerb::Pick));
        assert_eq!(RebaseVerb::parse("P"), Some(RebaseVerb::Pick));
        assert_eq!(RebaseVerb::parse("Squash"), Some(RebaseVerb::Squash));
        assert_eq!(RebaseVerb::parse("f"), Some(RebaseVerb::Fixup));
        assert_eq!(RebaseVerb::parse("REWORD"), Some(RebaseVerb::Reword));
        assert_eq!(RebaseVerb::parse("e"), Some(RebaseVerb::Edit));
        assert_eq!(RebaseVerb::parse("d"), Some(RebaseVerb::Drop));
        assert_eq!(RebaseVerb::parse("bogus"), None);
    }

    #[test]
    fn initial_todo_reverses_newest_first_log_into_oldest_first_picks() {
        let commits = vec![
            commit("cccccccccccccccccccccccccccccccccccccccc", "third"),
            commit("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "second"),
            commit("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "first"),
        ];
        let steps = initial_todo_from_log(&commits);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].subject, "first");
        assert_eq!(steps[1].subject, "second");
        assert_eq!(steps[2].subject, "third");
        assert!(steps.iter().all(|s| s.verb == RebaseVerb::Pick));
    }

    #[test]
    fn render_then_parse_round_trips_verb_and_sha() {
        let steps = vec![RebaseStep {
            verb: RebaseVerb::Pick,
            sha: "abcdef0123456789abcdef0123456789abcdef01".to_string(),
            subject: "fix bug".to_string(),
        }];
        let text = render_todo(&steps);
        assert_eq!(text, "pick abcdef01 fix bug");
        let parsed = parse_todo(&text);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].verb, RebaseVerb::Pick);
        assert_eq!(parsed[0].sha, "abcdef01");
        assert_eq!(parsed[0].subject, "fix bug");
    }

    #[test]
    fn parse_todo_skips_blank_and_comment_lines() {
        let steps = parse_todo("pick abc123 one\n\n# a comment\nsquash def456 two\n");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].verb, RebaseVerb::Pick);
        assert_eq!(steps[1].verb, RebaseVerb::Squash);
    }

    #[test]
    fn parse_todo_skips_unrecognized_verb_without_panicking() {
        let steps = parse_todo("bogus abc123 one\npick def456 two\n");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].sha, "def456");
    }

    #[test]
    fn parse_todo_deleted_line_simply_absent_matches_drop_semantics() {
        // Deleting a line from the buffer (the delete-idiom) produces the
        // same result as never having included that step at all.
        let with_all = parse_todo("pick a one\npick b two\npick c three\n");
        let with_middle_deleted = parse_todo("pick a one\npick c three\n");
        assert_eq!(with_all.len(), 3);
        assert_eq!(with_middle_deleted.len(), 2);
        assert_eq!(with_middle_deleted[0].sha, "a");
        assert_eq!(with_middle_deleted[1].sha, "c");
    }

    #[test]
    fn combine_squash_messages_joins_with_blank_line() {
        assert_eq!(
            combine_squash_messages("first commit", "second commit"),
            "first commit\n\nsecond commit"
        );
    }

    #[test]
    fn combine_squash_messages_handles_empty_incoming() {
        assert_eq!(combine_squash_messages("only message", ""), "only message");
    }

    #[test]
    fn combine_squash_messages_trims_trailing_and_leading_whitespace() {
        assert_eq!(
            combine_squash_messages("first\n\n", "  second  \n"),
            "first\n\nsecond"
        );
    }
}
