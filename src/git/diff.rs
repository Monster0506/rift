//! Parses unified `git diff --no-ext-diff -U<n>` output for staging and gutter signs.

use std::iter::Peekable;
use std::path::PathBuf;

/// Role of one line inside a [`Hunk`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
}

/// One line of hunk content, with its line number on each side it exists on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// Line text with the leading `+`/`-`/` ` marker stripped; no trailing newline.
    pub content: String,
    /// 1-based line number on the old side. `None` for a pure addition.
    pub old_lineno: Option<u32>,
    /// 1-based line number on the new side. `None` for a pure deletion.
    pub new_lineno: Option<u32>,
}

/// One `@@ -old_start,old_lines +new_start,new_lines @@` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// Optional trailing section-heading text after the second `@@`.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// One file's worth of a unified diff (the `diff --git ...` block through
/// its last hunk).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// `None` when the file doesn't exist on the old side (new file, or the
    /// `--- /dev/null` line was seen).
    pub old_path: Option<PathBuf>,
    /// `None` when the file doesn't exist on the new side (deleted file, or
    /// the `+++ /dev/null` line was seen).
    pub new_path: Option<PathBuf>,
    pub is_new_file: bool,
    pub is_deleted_file: bool,
    pub is_rename: bool,
    pub is_binary: bool,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    /// `similarity index NN%` from a rename/copy header, if present.
    pub similarity: Option<u8>,
    pub hunks: Vec<Hunk>,
}

/// One gutter-diff sign kind for a changed line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GutterSignKind {
    Add,
    Change,
    Delete,
}

/// Classify each hunk into 0-indexed buffer line(s) plus a sign kind. A pure addition signs every added line `Add`; a pure deletion signs one `Delete` marker at the line immediately before the deletion point (gitsigns.nvim convention; there's no added line to anchor to); a mixed hunk signs every line on the.
pub fn classify_gutter_signs(hunks: &[Hunk]) -> Vec<(usize, GutterSignKind)> {
    let mut signs = Vec::new();
    for hunk in hunks {
        let is_pure_deletion = hunk.new_lines == 0;
        if is_pure_deletion {
            let line = hunk.new_start.saturating_sub(1) as usize;
            signs.push((line, GutterSignKind::Delete));
            continue;
        }
        let kind = if hunk.old_lines == 0 {
            GutterSignKind::Add
        } else {
            GutterSignKind::Change
        };
        let start = hunk.new_start.saturating_sub(1) as usize;
        for i in 0..hunk.new_lines as usize {
            signs.push((start + i, kind));
        }
    }
    signs
}

/// Build a sub-hunk containing only the `+`/`-` lines at `selected` (indices into `hunk.lines`), for per-line staging: the same algorithm `git add -p` uses when you deselect part of a hunk. An unselected addition is dropped entirely (as if it were never added); an unselected deletion becomes context (as if it.
pub fn filter_hunk_to_lines(hunk: &Hunk, selected: &std::collections::HashSet<usize>) -> Hunk {
    let filtered: Vec<DiffLine> = hunk
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| match line.kind {
            DiffLineKind::Context => Some(line.clone()),
            DiffLineKind::Addition if selected.contains(&i) => Some(line.clone()),
            DiffLineKind::Addition => None,
            DiffLineKind::Deletion if selected.contains(&i) => Some(line.clone()),
            DiffLineKind::Deletion => Some(DiffLine {
                kind: DiffLineKind::Context,
                content: line.content.clone(),
                old_lineno: line.old_lineno,
                new_lineno: line.new_lineno,
            }),
        })
        .collect();
    let old_lines = filtered
        .iter()
        .filter(|l| matches!(l.kind, DiffLineKind::Context | DiffLineKind::Deletion))
        .count() as u32;
    let new_lines = filtered
        .iter()
        .filter(|l| matches!(l.kind, DiffLineKind::Context | DiffLineKind::Addition))
        .count() as u32;
    Hunk {
        old_start: hunk.old_start,
        old_lines,
        new_start: hunk.new_start,
        new_lines,
        header: hunk.header.clone(),
        lines: filtered,
    }
}

/// The contiguous run of non-context lines around `line_index` (inclusive), as indices into `hunk.lines`; the unit a single cursor line expands to for per-line staging. A lone `+`/`-` line stays a unit of one. A run that's purely deletions or purely additions (no opposite-kind neighbor, e.g. a brand-new file's.
pub fn diff_line_change_block(hunk: &Hunk, line_index: usize) -> std::collections::HashSet<usize> {
    let mut block = std::collections::HashSet::new();
    if hunk
        .lines
        .get(line_index)
        .is_none_or(|l| l.kind == DiffLineKind::Context)
    {
        return block;
    }
    let mut start = line_index;
    while start > 0 && hunk.lines[start - 1].kind != DiffLineKind::Context {
        start -= 1;
    }
    let mut end = line_index;
    while end + 1 < hunk.lines.len() && hunk.lines[end + 1].kind != DiffLineKind::Context {
        end += 1;
    }
    let run = &hunk.lines[start..=end];
    let is_mixed_replace = run.iter().any(|l| l.kind == DiffLineKind::Deletion)
        && run.iter().any(|l| l.kind == DiffLineKind::Addition);
    if is_mixed_replace {
        block.extend(start..=end);
    } else {
        block.insert(line_index);
    }
    block
}

/// Parse the full stdout of `git diff --no-ext-diff` (any number of files).
pub fn parse_unified_diff(input: &str) -> Vec<FileDiff> {
    let mut files = Vec::new();
    let mut lines = input.lines().peekable();

    while let Some(line) = lines.next() {
        if line.starts_with("diff --git ") {
            files.push(parse_one_file(line, &mut lines));
        }
    }

    files
}

fn parse_one_file<'a, I: Iterator<Item = &'a str>>(
    diff_git_line: &str,
    lines: &mut Peekable<I>,
) -> FileDiff {
    let (fallback_old, fallback_new) = parse_diff_git_paths(diff_git_line);

    let mut file = FileDiff {
        old_path: fallback_old,
        new_path: fallback_new,
        is_new_file: false,
        is_deleted_file: false,
        is_rename: false,
        is_binary: false,
        old_mode: None,
        new_mode: None,
        similarity: None,
        hunks: Vec::new(),
    };

    while let Some(&line) = lines.peek() {
        if line.starts_with("diff --git ") {
            break;
        }
        if let Some(hunk_start) = line.strip_prefix("@@ ") {
            if let Some(mut hunk) = parse_hunk_header(hunk_start) {
                lines.next();
                consume_hunk_body(&mut hunk, lines);
                file.hunks.push(hunk);
            } else {
                lines.next();
            }
            continue;
        }

        lines.next();
        apply_header_line(&mut file, line);
    }

    file
}

fn apply_header_line(file: &mut FileDiff, line: &str) {
    if let Some(mode) = line.strip_prefix("old mode ") {
        file.old_mode = Some(mode.to_string());
    } else if let Some(mode) = line.strip_prefix("new mode ") {
        file.new_mode = Some(mode.to_string());
    } else if let Some(mode) = line.strip_prefix("deleted file mode ") {
        file.is_deleted_file = true;
        file.old_mode = Some(mode.to_string());
    } else if let Some(mode) = line.strip_prefix("new file mode ") {
        file.is_new_file = true;
        file.new_mode = Some(mode.to_string());
    } else if let Some(path) = line.strip_prefix("rename from ") {
        file.is_rename = true;
        file.old_path = Some(PathBuf::from(path));
    } else if let Some(path) = line.strip_prefix("rename to ") {
        file.is_rename = true;
        file.new_path = Some(PathBuf::from(path));
    } else if let Some(path) = line.strip_prefix("copy from ") {
        file.old_path = Some(PathBuf::from(path));
    } else if let Some(path) = line.strip_prefix("copy to ") {
        file.new_path = Some(PathBuf::from(path));
    } else if let Some(pct) = line
        .strip_prefix("similarity index ")
        .and_then(|s| s.strip_suffix('%'))
    {
        file.similarity = pct.parse().ok();
    } else if line.starts_with("Binary files ") && line.ends_with(" differ") {
        file.is_binary = true;
    } else if let Some(path) = line.strip_prefix("--- ") {
        file.old_path = (path != "/dev/null").then(|| strip_ab_prefix(path));
    } else if let Some(path) = line.strip_prefix("+++ ") {
        file.new_path = (path != "/dev/null").then(|| strip_ab_prefix(path));
    }
    // "index <a>..<b> <mode>", "dissimilarity index", "GIT binary patch",
    // etc. carry no information this parser's consumers need yet.
}

fn strip_ab_prefix(path: &str) -> PathBuf {
    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path);
    PathBuf::from(path)
}

/// Best-effort split of `diff --git a/<old> b/<new>` into its two paths. Ambiguous when a path itself contains the literal substring `" b/"` (ambiguity inherent to the format for paths with spaces); the `---`, `+++`, and `rename from`/`rename to` lines are authoritative and override this fallback whenever.
fn parse_diff_git_paths(line: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    let rest = line.strip_prefix("diff --git ").unwrap_or(line);
    let Some(a_rest) = rest.strip_prefix("a/") else {
        return (None, None);
    };
    let Some(idx) = a_rest.find(" b/") else {
        return (None, None);
    };
    let old = &a_rest[..idx];
    let new = &a_rest[idx + " b/".len()..];
    (Some(PathBuf::from(old)), Some(PathBuf::from(new)))
}

/// Parse everything after `"@@ "` up to and including the closing `@@`.
fn parse_hunk_header(rest: &str) -> Option<Hunk> {
    let end = rest.find(" @@")?;
    let ranges = &rest[..end];
    let header = rest[end + " @@".len()..].trim_start().to_string();

    let mut parts = ranges.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let (old_start, old_lines) = parse_range(old)?;
    let (new_start, new_lines) = parse_range(new)?;

    Some(Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        header,
        lines: Vec::new(),
    })
}

/// Parse a `start[,length]` hunk-range component; `length` defaults to 1.
fn parse_range(s: &str) -> Option<(u32, u32)> {
    match s.split_once(',') {
        Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
        None => Some((s.parse().ok()?, 1)),
    }
}

fn consume_hunk_body<'a, I: Iterator<Item = &'a str>>(hunk: &mut Hunk, lines: &mut Peekable<I>) {
    let mut old_lineno = hunk.old_start;
    let mut new_lineno = hunk.new_start;

    while let Some(&line) = lines.peek() {
        if line.starts_with("@@ ") || line.starts_with("diff --git ") {
            break;
        }
        lines.next();

        if line.starts_with('\\') {
            // "\ No newline at end of file";  not a content line.
            continue;
        }

        let (kind, content) = if let Some(rest) = line.strip_prefix('+') {
            (DiffLineKind::Addition, rest)
        } else if let Some(rest) = line.strip_prefix('-') {
            (DiffLineKind::Deletion, rest)
        } else if let Some(rest) = line.strip_prefix(' ') {
            (DiffLineKind::Context, rest)
        } else if line.is_empty() {
            // A genuinely blank context line has no leading space once
            // `.lines()` has already stripped the newline.
            (DiffLineKind::Context, "")
        } else {
            // Not a valid hunk-body marker; malformed input, stop this hunk.
            break;
        };

        let (old_lineno_for_line, new_lineno_for_line) = match kind {
            DiffLineKind::Context => {
                let pair = (Some(old_lineno), Some(new_lineno));
                old_lineno += 1;
                new_lineno += 1;
                pair
            }
            DiffLineKind::Deletion => {
                let pair = (Some(old_lineno), None);
                old_lineno += 1;
                pair
            }
            DiffLineKind::Addition => {
                let pair = (None, Some(new_lineno));
                new_lineno += 1;
                pair
            }
        };

        hunk.lines.push(DiffLine {
            kind,
            content: content.to_string(),
            old_lineno: old_lineno_for_line,
            new_lineno: new_lineno_for_line,
        });
    }
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod diff_tests;
