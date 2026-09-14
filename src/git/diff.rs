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
mod tests {
    use super::*;

    #[test]
    fn parses_single_hunk_modification() {
        let input = "diff --git a/src/main.rs b/src/main.rs\n\
                      index e69de29..4b825dc 100644\n\
                      --- a/src/main.rs\n\
                      +++ b/src/main.rs\n\
                      @@ -1,3 +1,3 @@\n\
                      \x20fn main() {\n\
                      -    println!(\"old\");\n\
                      +    println!(\"new\");\n\
                      \x20}\n";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.old_path, Some(PathBuf::from("src/main.rs")));
        assert_eq!(file.new_path, Some(PathBuf::from("src/main.rs")));
        assert!(!file.is_new_file && !file.is_deleted_file && !file.is_rename);

        assert_eq!(file.hunks.len(), 1);
        let hunk = &file.hunks[0];
        assert_eq!(
            (
                hunk.old_start,
                hunk.old_lines,
                hunk.new_start,
                hunk.new_lines
            ),
            (1, 3, 1, 3)
        );
        assert_eq!(hunk.lines.len(), 4);

        assert_eq!(hunk.lines[0].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[0].content, "fn main() {");
        assert_eq!(hunk.lines[0].old_lineno, Some(1));
        assert_eq!(hunk.lines[0].new_lineno, Some(1));

        assert_eq!(hunk.lines[1].kind, DiffLineKind::Deletion);
        assert_eq!(hunk.lines[1].content, "    println!(\"old\");");
        assert_eq!(hunk.lines[1].old_lineno, Some(2));
        assert_eq!(hunk.lines[1].new_lineno, None);

        assert_eq!(hunk.lines[2].kind, DiffLineKind::Addition);
        assert_eq!(hunk.lines[2].content, "    println!(\"new\");");
        assert_eq!(hunk.lines[2].old_lineno, None);
        assert_eq!(hunk.lines[2].new_lineno, Some(2));

        assert_eq!(hunk.lines[3].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[3].old_lineno, Some(3));
        assert_eq!(hunk.lines[3].new_lineno, Some(3));
    }

    #[test]
    fn parses_new_file() {
        let input = "diff --git a/new.txt b/new.txt\n\
                      new file mode 100644\n\
                      index 0000000..e69de29\n\
                      --- /dev/null\n\
                      +++ b/new.txt\n\
                      @@ -0,0 +1,2 @@\n\
                      +hello\n\
                      +world\n";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert!(file.is_new_file);
        assert_eq!(file.new_mode.as_deref(), Some("100644"));
        assert_eq!(file.old_path, None);
        assert_eq!(file.new_path, Some(PathBuf::from("new.txt")));

        let hunk = &file.hunks[0];
        assert_eq!((hunk.old_start, hunk.old_lines), (0, 0));
        assert_eq!((hunk.new_start, hunk.new_lines), (1, 2));
        assert_eq!(hunk.lines[0].old_lineno, None);
        assert_eq!(hunk.lines[0].new_lineno, Some(1));
        assert_eq!(hunk.lines[1].new_lineno, Some(2));
    }

    #[test]
    fn parses_deleted_file() {
        let input = "diff --git a/gone.txt b/gone.txt\n\
                      deleted file mode 100644\n\
                      index e69de29..0000000\n\
                      --- a/gone.txt\n\
                      +++ /dev/null\n\
                      @@ -1,2 +0,0 @@\n\
                      -line1\n\
                      -line2\n";
        let files = parse_unified_diff(input);
        let file = &files[0];
        assert!(file.is_deleted_file);
        assert_eq!(file.old_mode.as_deref(), Some("100644"));
        assert_eq!(file.old_path, Some(PathBuf::from("gone.txt")));
        assert_eq!(file.new_path, None);

        let hunk = &file.hunks[0];
        assert_eq!(hunk.lines.len(), 2);
        assert_eq!(hunk.lines[0].old_lineno, Some(1));
        assert_eq!(hunk.lines[1].old_lineno, Some(2));
        assert!(hunk.lines.iter().all(|l| l.new_lineno.is_none()));
    }

    #[test]
    fn parses_rename_with_similarity_and_modification() {
        let input = "diff --git a/old_name.rs b/new_name.rs\n\
                      similarity index 92%\n\
                      rename from old_name.rs\n\
                      rename to new_name.rs\n\
                      index abc..def 100644\n\
                      --- a/old_name.rs\n\
                      +++ b/new_name.rs\n\
                      @@ -1 +1 @@\n\
                      -old content\n\
                      +new content\n";
        let files = parse_unified_diff(input);
        let file = &files[0];
        assert!(file.is_rename);
        assert_eq!(file.similarity, Some(92));
        assert_eq!(file.old_path, Some(PathBuf::from("old_name.rs")));
        assert_eq!(file.new_path, Some(PathBuf::from("new_name.rs")));
        assert_eq!(file.hunks[0].old_start, 1);
        assert_eq!(file.hunks[0].old_lines, 1);
    }

    #[test]
    fn parses_binary_file_diff_with_no_hunks() {
        let input = "diff --git a/image.png b/image.png\n\
                      index abc123..def456 100644\n\
                      Binary files a/image.png and b/image.png differ\n";
        let files = parse_unified_diff(input);
        let file = &files[0];
        assert!(file.is_binary);
        assert!(file.hunks.is_empty());
    }

    #[test]
    fn parses_multiple_files_in_one_diff() {
        let input = "diff --git a/a.txt b/a.txt\n\
                      --- a/a.txt\n\
                      +++ b/a.txt\n\
                      @@ -1 +1 @@\n\
                      -old\n\
                      +new\n\
                      diff --git a/b.txt b/b.txt\n\
                      new file mode 100644\n\
                      --- /dev/null\n\
                      +++ b/b.txt\n\
                      @@ -0,0 +1 @@\n\
                      +created\n";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].new_path, Some(PathBuf::from("a.txt")));
        assert_eq!(files[1].new_path, Some(PathBuf::from("b.txt")));
        assert!(files[1].is_new_file);
    }

    #[test]
    fn hunk_header_section_text_is_captured() {
        let input = "diff --git a/f.rs b/f.rs\n\
                      --- a/f.rs\n\
                      +++ b/f.rs\n\
                      @@ -10,2 +10,2 @@ fn main() {\n\
                      \x20a\n\
                      \x20b\n";
        let files = parse_unified_diff(input);
        assert_eq!(files[0].hunks[0].header, "fn main() {");
    }

    #[test]
    fn empty_input_yields_no_files() {
        assert!(parse_unified_diff("").is_empty());
    }

    #[test]
    fn no_newline_at_end_of_file_marker_is_ignored() {
        let input = "diff --git a/f.txt b/f.txt\n\
                      --- a/f.txt\n\
                      +++ b/f.txt\n\
                      @@ -1 +1 @@\n\
                      -old\n\
                      \\ No newline at end of file\n\
                      +new\n\
                      \\ No newline at end of file\n";
        let files = parse_unified_diff(input);
        assert_eq!(files[0].hunks[0].lines.len(), 2);
    }

    fn hunk(old_start: u32, old_lines: u32, new_start: u32, new_lines: u32) -> Hunk {
        Hunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
            header: String::new(),
            lines: Vec::new(),
        }
    }

    #[test]
    fn classify_gutter_signs_pure_addition_signs_every_added_line() {
        let signs = classify_gutter_signs(&[hunk(5, 0, 6, 3)]);
        assert_eq!(
            signs,
            vec![
                (5, GutterSignKind::Add),
                (6, GutterSignKind::Add),
                (7, GutterSignKind::Add),
            ]
        );
    }

    #[test]
    fn classify_gutter_signs_pure_deletion_signs_one_marker_before_the_gap() {
        // Old had 2 lines removed starting at old line 10; new side has 0 lines
        // (new_start conventionally points just before the gap, here line 9).
        let signs = classify_gutter_signs(&[hunk(10, 2, 9, 0)]);
        assert_eq!(signs, vec![(8, GutterSignKind::Delete)]);
    }

    #[test]
    fn classify_gutter_signs_deletion_at_start_of_file_clamps_to_zero() {
        let signs = classify_gutter_signs(&[hunk(1, 1, 0, 0)]);
        assert_eq!(signs, vec![(0, GutterSignKind::Delete)]);
    }

    #[test]
    fn classify_gutter_signs_mixed_hunk_signs_change_for_every_new_line() {
        let signs = classify_gutter_signs(&[hunk(3, 1, 3, 2)]);
        assert_eq!(
            signs,
            vec![(2, GutterSignKind::Change), (3, GutterSignKind::Change)]
        );
    }

    #[test]
    fn classify_gutter_signs_multiple_hunks_are_independent() {
        let signs = classify_gutter_signs(&[hunk(1, 0, 1, 1), hunk(20, 1, 19, 0)]);
        assert_eq!(
            signs,
            vec![(0, GutterSignKind::Add), (18, GutterSignKind::Delete)]
        );
    }

    fn diff_line(kind: DiffLineKind, content: &str) -> DiffLine {
        DiffLine {
            kind,
            content: content.to_string(),
            old_lineno: None,
            new_lineno: None,
        }
    }

    #[test]
    fn filter_hunk_to_lines_keeps_only_the_selected_addition() {
        // @@ -5,3 +5,5 @@; two additions among context; select only the first.
        let h = Hunk {
            old_start: 5,
            old_lines: 3,
            new_start: 5,
            new_lines: 5,
            header: String::new(),
            lines: vec![
                diff_line(DiffLineKind::Context, "a"),
                diff_line(DiffLineKind::Addition, "added one"),
                diff_line(DiffLineKind::Addition, "added two"),
                diff_line(DiffLineKind::Context, "b"),
            ],
        };
        let selected: std::collections::HashSet<usize> = [1].into_iter().collect();
        let sub = filter_hunk_to_lines(&h, &selected);
        assert_eq!(sub.old_start, 5);
        assert_eq!(sub.new_start, 5);
        assert_eq!(sub.old_lines, 2, "2 context lines, no deletions");
        assert_eq!(sub.new_lines, 3, "2 context + 1 selected addition");
        assert_eq!(
            sub.lines,
            vec![
                diff_line(DiffLineKind::Context, "a"),
                diff_line(DiffLineKind::Addition, "added one"),
                diff_line(DiffLineKind::Context, "b"),
            ]
        );
    }

    #[test]
    fn filter_hunk_to_lines_turns_an_unselected_deletion_into_context() {
        let h = Hunk {
            old_start: 1,
            old_lines: 3,
            new_start: 1,
            new_lines: 1,
            header: String::new(),
            lines: vec![
                diff_line(DiffLineKind::Deletion, "removed one"),
                diff_line(DiffLineKind::Deletion, "removed two"),
                diff_line(DiffLineKind::Context, "kept"),
            ],
        };
        // Select only the second deletion; the first must survive as context.
        let selected: std::collections::HashSet<usize> = [1].into_iter().collect();
        let sub = filter_hunk_to_lines(&h, &selected);
        assert_eq!(
            sub.lines[0].kind,
            DiffLineKind::Context,
            "unselected deletion becomes context"
        );
        assert_eq!(sub.lines[0].content, "removed one");
        assert_eq!(
            sub.lines[1].kind,
            DiffLineKind::Deletion,
            "selected deletion stays a deletion"
        );
        assert_eq!(sub.old_lines, 3, "all three lines exist on the old side");
        assert_eq!(
            sub.new_lines, 2,
            "context 'removed one' + context 'kept' on the new side"
        );
    }

    #[test]
    fn filter_hunk_to_lines_with_nothing_selected_is_pure_context() {
        let h = Hunk {
            old_start: 10,
            old_lines: 1,
            new_start: 10,
            new_lines: 2,
            header: String::new(),
            lines: vec![
                diff_line(DiffLineKind::Deletion, "old"),
                diff_line(DiffLineKind::Addition, "new"),
            ],
        };
        let sub = filter_hunk_to_lines(&h, &std::collections::HashSet::new());
        assert!(
            sub.lines.iter().all(|l| l.kind == DiffLineKind::Context),
            "no selection must produce a no-op sub-patch: {:?}",
            sub.lines
        );
        assert_eq!(sub.old_lines, 1);
        assert_eq!(
            sub.new_lines, 1,
            "the unselected addition is dropped entirely"
        );
    }

    #[test]
    fn diff_line_change_block_expands_an_adjacent_replace_pair() {
        // [-a, +A, ctx b, ctx c, -d, +D, ctx e, ctx f]
        let h = Hunk {
            old_start: 1,
            old_lines: 6,
            new_start: 1,
            new_lines: 6,
            header: String::new(),
            lines: vec![
                diff_line(DiffLineKind::Deletion, "a"),
                diff_line(DiffLineKind::Addition, "A"),
                diff_line(DiffLineKind::Context, "b"),
                diff_line(DiffLineKind::Context, "c"),
                diff_line(DiffLineKind::Deletion, "d"),
                diff_line(DiffLineKind::Addition, "D"),
                diff_line(DiffLineKind::Context, "e"),
                diff_line(DiffLineKind::Context, "f"),
            ],
        };
        // Cursor on the '+A' line (index 1) must pull in its paired '-a' too.
        let block = diff_line_change_block(&h, 1);
        assert_eq!(block, [0, 1].into_iter().collect());
        // Cursor on the '-d' line (index 4) must pull in its paired '+D' too.
        let block = diff_line_change_block(&h, 4);
        assert_eq!(block, [4, 5].into_iter().collect());
    }

    #[test]
    fn diff_line_change_block_expands_a_longer_replaced_run() {
        // A 3-line deletion replaced by a 2-line addition, all one block.
        let h = Hunk {
            old_start: 1,
            old_lines: 3,
            new_start: 1,
            new_lines: 2,
            header: String::new(),
            lines: vec![
                diff_line(DiffLineKind::Deletion, "old1"),
                diff_line(DiffLineKind::Deletion, "old2"),
                diff_line(DiffLineKind::Deletion, "old3"),
                diff_line(DiffLineKind::Addition, "new1"),
                diff_line(DiffLineKind::Addition, "new2"),
            ],
        };
        for i in 0..5 {
            assert_eq!(
                diff_line_change_block(&h, i),
                (0..5).collect(),
                "every line in the run must expand to the whole block"
            );
        }
    }

    #[test]
    fn diff_line_change_block_keeps_a_pure_addition_run_per_line() {
        // A brand-new file's diff (or any pure insertion with no paired deletion nearby): every '+' line is an independent, complete change, so each stays its own singleton block.
        let h = Hunk {
            old_start: 0,
            old_lines: 0,
            new_start: 1,
            new_lines: 3,
            header: String::new(),
            lines: vec![
                diff_line(DiffLineKind::Addition, "one"),
                diff_line(DiffLineKind::Addition, "two"),
                diff_line(DiffLineKind::Addition, "three"),
            ],
        };
        for i in 0..3 {
            assert_eq!(diff_line_change_block(&h, i), [i].into_iter().collect());
        }
    }

    #[test]
    fn diff_line_change_block_keeps_a_pure_deletion_run_per_line() {
        let h = Hunk {
            old_start: 1,
            old_lines: 3,
            new_start: 0,
            new_lines: 0,
            header: String::new(),
            lines: vec![
                diff_line(DiffLineKind::Deletion, "one"),
                diff_line(DiffLineKind::Deletion, "two"),
                diff_line(DiffLineKind::Deletion, "three"),
            ],
        };
        for i in 0..3 {
            assert_eq!(diff_line_change_block(&h, i), [i].into_iter().collect());
        }
    }

    #[test]
    fn diff_line_change_block_on_a_context_line_is_empty() {
        let h = Hunk {
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
            header: String::new(),
            lines: vec![diff_line(DiffLineKind::Context, "unchanged")],
        };
        assert!(diff_line_change_block(&h, 0).is_empty());
    }
}
