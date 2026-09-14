//! Parses `git status --porcelain=v2 --branch --untracked-files=all` output without subprocesses.

use std::path::PathBuf;

/// Ahead/behind + identity of the current branch, from the `# branch.*`
/// header lines porcelain v2 emits when invoked with `--branch`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BranchInfo {
    /// HEAD commit. `None` on a fresh repo with no commits yet (`(initial)`).
    pub oid: Option<String>,
    /// Current branch name. `None` in detached-HEAD state.
    pub head: Option<String>,
    /// Configured upstream ref, if any.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// One side (`X` = index-vs-HEAD, `Y` = worktree-vs-index) of a changed entry's status code. Also reused for unmerged entries' `XY`, where the only letters git emits are `U`/`A`/`D`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Unmodified,
    Modified,
    TypeChanged,
    Added,
    Deleted,
    Renamed,
    Copied,
    UpdatedUnmerged,
}

impl FileState {
    fn from_char(c: u8) -> Option<Self> {
        match c {
            b'.' => Some(Self::Unmodified),
            b'M' => Some(Self::Modified),
            b'T' => Some(Self::TypeChanged),
            b'A' => Some(Self::Added),
            b'D' => Some(Self::Deleted),
            b'R' => Some(Self::Renamed),
            b'C' => Some(Self::Copied),
            b'U' => Some(Self::UpdatedUnmerged),
            _ => None,
        }
    }
}

/// What kind of porcelain-v2 record produced a [`StatusEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// `1 ...` ordinary changed entry.
    Ordinary,
    /// `2 ...` renamed or copied entry. `score` is e.g. `"R100"`/`"C75"`.
    RenamedOrCopied { score: String },
    /// `u ...` unmerged (conflicted) entry.
    Unmerged,
    /// `? ...` untracked path.
    Untracked,
    /// `! ...` ignored path (only emitted with `--ignored`).
    Ignored,
}

/// One file-level record from a porcelain v2 status listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: PathBuf,
    /// Previous path, populated only for a rename/copy record.
    pub orig_path: Option<PathBuf>,
    /// `X`: staged (index-vs-HEAD) state. `Unmodified` for untracked/ignored.
    pub index_state: FileState,
    /// `Y`: unstaged (worktree-vs-index) state. `Unmodified` for untracked/ignored.
    pub worktree_state: FileState,
    pub kind: EntryKind,
}

impl StatusEntry {
    pub fn is_untracked(&self) -> bool {
        matches!(self.kind, EntryKind::Untracked)
    }

    pub fn is_ignored(&self) -> bool {
        matches!(self.kind, EntryKind::Ignored)
    }

    pub fn is_unmerged(&self) -> bool {
        matches!(self.kind, EntryKind::Unmerged)
    }

    /// Whether this entry belongs under a status buffer's "Staged" section.
    pub fn is_staged(&self) -> bool {
        !self.is_untracked()
            && !self.is_ignored()
            && !self.is_unmerged()
            && self.index_state != FileState::Unmodified
    }

    /// Whether this entry belongs under a status buffer's "Unstaged" section.
    pub fn is_unstaged(&self) -> bool {
        !self.is_untracked()
            && !self.is_ignored()
            && !self.is_unmerged()
            && self.worktree_state != FileState::Unmodified
    }
}

/// A fully parsed `git status --porcelain=v2 --branch` listing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusSnapshot {
    pub branch: BranchInfo,
    pub entries: Vec<StatusEntry>,
}

/// Parse porcelain v2 stdout. Malformed or unrecognized lines are skipped rather than failing the whole parse: status output is advisory, and one unexpected line from a newer/older git shouldn't blank the buffer.
pub fn parse_status(input: &str) -> StatusSnapshot {
    let mut snapshot = StatusSnapshot::default();

    for line in input.lines() {
        if let Some(rest) = line.strip_prefix("# branch.") {
            parse_branch_header(rest, &mut snapshot.branch);
        } else if let Some(entry) = parse_entry_line(line) {
            snapshot.entries.push(entry);
        }
    }

    snapshot
}

fn parse_branch_header(rest: &str, branch: &mut BranchInfo) {
    let Some((key, value)) = rest.split_once(' ') else {
        return;
    };
    match key {
        "oid" => branch.oid = (value != "(initial)").then(|| value.to_string()),
        "head" => branch.head = (value != "(detached)").then(|| value.to_string()),
        "upstream" => branch.upstream = Some(value.to_string()),
        "ab" => {
            // "+<ahead> -<behind>"
            for part in value.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    branch.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    branch.behind = n.parse().unwrap_or(0);
                }
            }
        }
        _ => {}
    }
}

fn parse_entry_line(line: &str) -> Option<StatusEntry> {
    let mut fields = line.splitn(2, ' ');
    let tag = fields.next()?;
    let rest = fields.next().unwrap_or("");

    match tag {
        "1" => parse_ordinary(rest),
        "2" => parse_renamed(rest),
        "u" => parse_unmerged(rest),
        "?" => Some(bare_entry(rest, EntryKind::Untracked)),
        "!" => Some(bare_entry(rest, EntryKind::Ignored)),
        _ => None,
    }
}

fn bare_entry(path: &str, kind: EntryKind) -> StatusEntry {
    StatusEntry {
        path: PathBuf::from(path),
        orig_path: None,
        index_state: FileState::Unmodified,
        worktree_state: FileState::Unmodified,
        kind,
    }
}

/// Split off `count` leading space-delimited fixed fields, returning them
/// plus whatever remains (the path, or `path\torigPath` for renames).
fn split_fixed_fields(rest: &str, count: usize) -> Option<(Vec<&str>, &str)> {
    let mut fields = Vec::with_capacity(count);
    let mut remainder = rest;
    for _ in 0..count {
        let (field, tail) = remainder.split_once(' ')?;
        fields.push(field);
        remainder = tail;
    }
    Some((fields, remainder))
}

fn parse_xy(xy: &str) -> Option<(FileState, FileState)> {
    let bytes = xy.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    Some((
        FileState::from_char(bytes[0])?,
        FileState::from_char(bytes[1])?,
    ))
}

fn parse_ordinary(rest: &str) -> Option<StatusEntry> {
    // XY sub mH mI mW hH hI path
    let (fields, path) = split_fixed_fields(rest, 7)?;
    let (index_state, worktree_state) = parse_xy(fields[0])?;
    Some(StatusEntry {
        path: PathBuf::from(path),
        orig_path: None,
        index_state,
        worktree_state,
        kind: EntryKind::Ordinary,
    })
}

fn parse_renamed(rest: &str) -> Option<StatusEntry> {
    // XY sub mH mI mW hH hI Xscore path\torigPath
    let (fields, tail) = split_fixed_fields(rest, 8)?;
    let (index_state, worktree_state) = parse_xy(fields[0])?;
    let score = fields[7].to_string();
    let (path, orig_path) = tail.split_once('\t')?;
    Some(StatusEntry {
        path: PathBuf::from(path),
        orig_path: Some(PathBuf::from(orig_path)),
        index_state,
        worktree_state,
        kind: EntryKind::RenamedOrCopied { score },
    })
}

fn parse_unmerged(rest: &str) -> Option<StatusEntry> {
    // XY sub m1 m2 m3 mW h1 h2 h3 path
    let (fields, path) = split_fixed_fields(rest, 9)?;
    let (index_state, worktree_state) = parse_xy(fields[0])?;
    Some(StatusEntry {
        path: PathBuf::from(path),
        orig_path: None,
        index_state,
        worktree_state,
        kind: EntryKind::Unmerged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_header_with_upstream_and_ahead_behind() {
        let snapshot = parse_status(
            "# branch.oid abc123\n\
             # branch.head main\n\
             # branch.upstream origin/main\n\
             # branch.ab +2 -1\n",
        );
        assert_eq!(snapshot.branch.oid.as_deref(), Some("abc123"));
        assert_eq!(snapshot.branch.head.as_deref(), Some("main"));
        assert_eq!(snapshot.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!(snapshot.branch.ahead, 2);
        assert_eq!(snapshot.branch.behind, 1);
    }

    #[test]
    fn parses_initial_commit_and_detached_head_as_none() {
        let snapshot = parse_status("# branch.oid (initial)\n# branch.head (detached)\n");
        assert_eq!(snapshot.branch.oid, None);
        assert_eq!(snapshot.branch.head, None);
    }

    #[test]
    fn parses_ordinary_staged_and_unstaged_entries() {
        let snapshot = parse_status(
            "1 M. N... 100644 100644 100644 aaa bbb staged.rs\n\
             1 .M N... 100644 100644 100644 aaa bbb unstaged.rs\n\
             1 MM N... 100644 100644 100644 aaa bbb both.rs\n",
        );
        assert_eq!(snapshot.entries.len(), 3);

        let staged = &snapshot.entries[0];
        assert_eq!(staged.path, PathBuf::from("staged.rs"));
        assert_eq!(staged.index_state, FileState::Modified);
        assert_eq!(staged.worktree_state, FileState::Unmodified);
        assert!(staged.is_staged());
        assert!(!staged.is_unstaged());

        let unstaged = &snapshot.entries[1];
        assert!(!unstaged.is_staged());
        assert!(unstaged.is_unstaged());

        let both = &snapshot.entries[2];
        assert!(both.is_staged());
        assert!(both.is_unstaged());
    }

    #[test]
    fn parses_untracked_and_ignored_paths_with_spaces() {
        let snapshot = parse_status(
            "? some file with spaces.txt\n\
             ! build/\n",
        );
        assert_eq!(snapshot.entries.len(), 2);
        assert!(snapshot.entries[0].is_untracked());
        assert_eq!(
            snapshot.entries[0].path,
            PathBuf::from("some file with spaces.txt")
        );
        assert!(snapshot.entries[1].is_ignored());
        assert_eq!(snapshot.entries[1].path, PathBuf::from("build/"));
    }

    #[test]
    fn parses_renamed_entry_with_score_and_orig_path() {
        let snapshot =
            parse_status("2 R. N... 100644 100644 100644 aaa bbb R100 new_name.rs\told_name.rs\n");
        assert_eq!(snapshot.entries.len(), 1);
        let entry = &snapshot.entries[0];
        assert_eq!(entry.path, PathBuf::from("new_name.rs"));
        assert_eq!(entry.orig_path, Some(PathBuf::from("old_name.rs")));
        assert_eq!(entry.index_state, FileState::Renamed);
        assert_eq!(
            entry.kind,
            EntryKind::RenamedOrCopied {
                score: "R100".to_string()
            }
        );
    }

    #[test]
    fn parses_unmerged_entry() {
        let snapshot =
            parse_status("u UU N... 100644 100644 100644 100644 aaa bbb ccc conflicted.txt\n");
        assert_eq!(snapshot.entries.len(), 1);
        let entry = &snapshot.entries[0];
        assert!(entry.is_unmerged());
        assert!(!entry.is_staged());
        assert!(!entry.is_unstaged());
        assert_eq!(entry.index_state, FileState::UpdatedUnmerged);
        assert_eq!(entry.worktree_state, FileState::UpdatedUnmerged);
    }

    #[test]
    fn skips_unrecognized_lines_without_panicking() {
        let snapshot = parse_status("# some future header\ngarbage line\n? real.txt\n");
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].path, PathBuf::from("real.txt"));
    }

    #[test]
    fn empty_input_yields_default_snapshot() {
        let snapshot = parse_status("");
        assert_eq!(snapshot, StatusSnapshot::default());
    }
}
