//! Parses porcelain blame output for a file or range. The parser handles commit metadata and source lines.

use std::collections::HashMap;
use std::sync::Arc;

/// Metadata for one commit as reported by blame. Shared by every [`BlameLine`] attributed to it; behind an `Arc` so a file with a few authors and thousands of lines doesn't pay for thousands of copies of each author's name/email/summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameCommit {
    pub sha: String,
    pub author: String,
    pub author_mail: String,
    pub author_time: i64,
    pub author_tz: String,
    pub committer: String,
    pub committer_mail: String,
    pub committer_time: i64,
    pub committer_tz: String,
    pub summary: String,
    /// `previous <sha> <filename>`: the commit/path this content was last changed from, absent for a commit that introduced the line (or for a `--boundary` root shown without one).
    pub previous: Option<(String, String)>,
    /// Set when git printed a bare `boundary` marker line for this commit
    /// (a `--boundary`-limited ancestor at the edge of the walked range).
    pub boundary: bool,
}

/// One final-file line of a blame listing, attributed to the commit that
/// last touched it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub commit: Arc<BlameCommit>,
    pub orig_line: u32,
    pub final_line: u32,
    pub content: String,
}

/// Mutable accumulator for a commit's metadata lines while a new group is being read. Every field defaults to empty/zero/false so truncated input (a commit whose metadata block was cut short) still produces a usable, non-panicking [`BlameCommit`] rather than failing the whole parse.
#[derive(Default)]
struct CommitBuilder {
    author: String,
    author_mail: String,
    author_time: i64,
    author_tz: String,
    committer: String,
    committer_mail: String,
    committer_time: i64,
    committer_tz: String,
    summary: String,
    previous: Option<(String, String)>,
    boundary: bool,
}

impl CommitBuilder {
    /// Apply one metadata line (everything between the header and `filename`). Unrecognized lines; a future git version's addition; are ignored rather than treated as a parse error.
    fn apply(&mut self, line: &str) {
        if let Some(v) = line.strip_prefix("author-mail ") {
            self.author_mail = v.to_string();
        } else if let Some(v) = line.strip_prefix("author-time ") {
            self.author_time = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("author-tz ") {
            self.author_tz = v.to_string();
        } else if let Some(v) = line.strip_prefix("author ") {
            self.author = v.to_string();
        } else if let Some(v) = line.strip_prefix("committer-mail ") {
            self.committer_mail = v.to_string();
        } else if let Some(v) = line.strip_prefix("committer-time ") {
            self.committer_time = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("committer-tz ") {
            self.committer_tz = v.to_string();
        } else if let Some(v) = line.strip_prefix("committer ") {
            self.committer = v.to_string();
        } else if let Some(v) = line.strip_prefix("summary ") {
            self.summary = v.to_string();
        } else if let Some(v) = line.strip_prefix("previous ") {
            if let Some((sha, filename)) = v.split_once(' ') {
                self.previous = Some((sha.to_string(), filename.to_string()));
            }
        } else if line == "boundary" {
            self.boundary = true;
        }
    }

    fn build(self, sha: String) -> BlameCommit {
        BlameCommit {
            sha,
            author: self.author,
            author_mail: self.author_mail,
            author_time: self.author_time,
            author_tz: self.author_tz,
            committer: self.committer,
            committer_mail: self.committer_mail,
            committer_time: self.committer_time,
            committer_tz: self.committer_tz,
            summary: self.summary,
            previous: self.previous,
            boundary: self.boundary,
        }
    }
}

/// Parse a header line (`<40-hex-sha> <orig-line> <final-line> [<n>]`) into `(sha, orig_line, final_line)`, ignoring the optional trailing group-size field. `None` for anything else; in well-formed blame output the only other line shapes are metadata (`key value`), `filename`, and the single tab-prefixed.
fn parse_header(line: &str) -> Option<(String, u32, u32)> {
    let mut parts = line.split(' ');
    let sha = parts.next()?;
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let orig_line: u32 = parts.next()?.parse().ok()?;
    let final_line: u32 = parts.next()?.parse().ok()?;
    Some((sha.to_string(), orig_line, final_line))
}

/// Parse the full stdout of `git blame --porcelain`. Malformed or truncated input degrades gracefully (missing fields default to empty/zero) rather than panicking; blame is advisory display data, not something worth failing the whole request over.
pub fn parse_blame(input: &str) -> Vec<BlameLine> {
    let lines: Vec<&str> = input.lines().collect();
    let mut seen: HashMap<String, Arc<BlameCommit>> = HashMap::new();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let Some((sha, orig_line, final_line)) = parse_header(lines[i]) else {
            // Not a header where one was expected; skip forward rather than
            // desyncing the whole rest of the parse on one bad line.
            i += 1;
            continue;
        };
        i += 1;

        let commit = if let Some(cached) = seen.get(&sha) {
            // Repeat mention: no metadata block, straight to `filename`.
            if i < lines.len() && lines[i].starts_with("filename ") {
                i += 1;
            }
            cached.clone()
        } else {
            let mut builder = CommitBuilder::default();
            while i < lines.len() {
                let line = lines[i];
                if line.starts_with("filename ") {
                    i += 1;
                    break;
                }
                i += 1;
                builder.apply(line);
            }
            let commit = Arc::new(builder.build(sha.clone()));
            seen.insert(sha, commit.clone());
            commit
        };

        if i < lines.len() {
            let content_line = lines[i];
            i += 1;
            let content = content_line.strip_prefix('\t').unwrap_or(content_line);
            result.push(BlameLine {
                commit,
                orig_line,
                final_line,
                content: content.to_string(),
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn parses_two_lines_with_different_commits() {
        let input = format!(
            "{SHA_A} 1 1 1\n\
             author Alice\n\
             author-mail <alice@example.com>\n\
             author-time 1000000000\n\
             author-tz +0000\n\
             committer Alice\n\
             committer-mail <alice@example.com>\n\
             committer-time 1000000000\n\
             committer-tz +0000\n\
             summary First commit\n\
             filename src/main.rs\n\
             \tfn main() {{}}\n\
             {SHA_B} 2 2 1\n\
             author Bob\n\
             author-mail <bob@example.com>\n\
             author-time 1000000100\n\
             author-tz +0000\n\
             committer Bob\n\
             committer-mail <bob@example.com>\n\
             committer-time 1000000100\n\
             committer-tz +0000\n\
             summary Second commit\n\
             filename src/main.rs\n\
             \tfn helper() {{}}\n"
        );
        let lines = parse_blame(&input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].commit.sha, SHA_A);
        assert_eq!(lines[0].commit.author, "Alice");
        assert_eq!(lines[0].content, "fn main() {}");
        assert_eq!(lines[1].commit.sha, SHA_B);
        assert_eq!(lines[1].commit.author, "Bob");
        assert_eq!(lines[1].content, "fn helper() {}");
    }

    #[test]
    fn same_commit_group_shares_metadata_across_lines() {
        let input = format!(
            "{SHA_A} 1 1 2\n\
             author Alice\n\
             author-mail <alice@example.com>\n\
             author-time 1000000000\n\
             author-tz +0000\n\
             committer Alice\n\
             committer-mail <alice@example.com>\n\
             committer-time 1000000000\n\
             committer-tz +0000\n\
             summary Batch commit\n\
             filename src/lib.rs\n\
             \tline one\n\
             {SHA_A} 2 2\n\
             filename src/lib.rs\n\
             \tline two\n"
        );
        let lines = parse_blame(&input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].content, "line one");
        assert_eq!(lines[1].content, "line two");
        // Same underlying metadata, cheaply shared rather than re-parsed.
        assert_eq!(lines[0].commit, lines[1].commit);
        assert!(Arc::ptr_eq(&lines[0].commit, &lines[1].commit));
        assert_eq!(lines[1].commit.author, "Alice");
        assert_eq!(lines[1].commit.summary, "Batch commit");
    }

    #[test]
    fn parses_previous_field() {
        let input = format!(
            "{SHA_A} 1 1 1\n\
             author Alice\n\
             author-mail <alice@example.com>\n\
             author-time 1000000000\n\
             author-tz +0000\n\
             committer Alice\n\
             committer-mail <alice@example.com>\n\
             committer-time 1000000000\n\
             committer-tz +0000\n\
             summary Follow-up\n\
             previous {SHA_B} old_name.rs\n\
             filename new_name.rs\n\
             \tcontent\n"
        );
        let lines = parse_blame(&input);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].commit.previous,
            Some((SHA_B.to_string(), "old_name.rs".to_string()))
        );
    }

    #[test]
    fn boundary_marker_sets_flag_only_when_present() {
        let with_boundary = format!(
            "{SHA_A} 1 1 1\n\
             author Alice\n\
             author-mail <alice@example.com>\n\
             author-time 1000000000\n\
             author-tz +0000\n\
             committer Alice\n\
             committer-mail <alice@example.com>\n\
             committer-time 1000000000\n\
             committer-tz +0000\n\
             summary Root\n\
             boundary\n\
             filename file.rs\n\
             \tcontent\n"
        );
        let without_boundary = format!(
            "{SHA_B} 1 1 1\n\
             author Bob\n\
             author-mail <bob@example.com>\n\
             author-time 1000000000\n\
             author-tz +0000\n\
             committer Bob\n\
             committer-mail <bob@example.com>\n\
             committer-time 1000000000\n\
             committer-tz +0000\n\
             summary Not root\n\
             filename file.rs\n\
             \tcontent\n"
        );
        assert!(parse_blame(&with_boundary)[0].commit.boundary);
        assert!(!parse_blame(&without_boundary)[0].commit.boundary);
    }

    #[test]
    fn content_with_leading_tab_preserves_inner_tab() {
        let input = format!(
            "{SHA_A} 1 1 1\n\
             author Alice\n\
             author-mail <alice@example.com>\n\
             author-time 1000000000\n\
             author-tz +0000\n\
             committer Alice\n\
             committer-mail <alice@example.com>\n\
             committer-time 1000000000\n\
             committer-tz +0000\n\
             summary Indented\n\
             filename file.rs\n\
             \t\ttabbed code\n"
        );
        let lines = parse_blame(&input);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].content, "\ttabbed code");
    }

    #[test]
    fn empty_input_yields_empty_vec() {
        assert_eq!(parse_blame(""), Vec::new());
    }

    #[test]
    fn parses_author_and_committer_time_as_i64() {
        let input = format!(
            "{SHA_A} 1 1 1\n\
             author Alice\n\
             author-mail <alice@example.com>\n\
             author-time 1700000000\n\
             author-tz +0000\n\
             committer Bob\n\
             committer-mail <bob@example.com>\n\
             committer-time 1700000500\n\
             committer-tz -0500\n\
             summary Times\n\
             filename file.rs\n\
             \tcontent\n"
        );
        let lines = parse_blame(&input);
        assert_eq!(lines[0].commit.author_time, 1_700_000_000_i64);
        assert_eq!(lines[0].commit.committer_time, 1_700_000_500_i64);
    }

    #[test]
    fn tracks_many_distinct_commits_across_repeats() {
        // sha A appears, then B, then A again (repeat, no metadata), then a third distinct sha C; the seen-map must keep each of A/B/C's metadata independently rather than collapsing to the last-seen one.
        let input = format!(
            "{SHA_A} 1 1 1\n\
             author Alice\n\
             author-mail <alice@example.com>\n\
             author-time 1000000000\n\
             author-tz +0000\n\
             committer Alice\n\
             committer-mail <alice@example.com>\n\
             committer-time 1000000000\n\
             committer-tz +0000\n\
             summary A commit\n\
             filename file.rs\n\
             \tline A\n\
             {SHA_B} 2 2 1\n\
             author Bob\n\
             author-mail <bob@example.com>\n\
             author-time 1000000100\n\
             author-tz +0000\n\
             committer Bob\n\
             committer-mail <bob@example.com>\n\
             committer-time 1000000100\n\
             committer-tz +0000\n\
             summary B commit\n\
             filename file.rs\n\
             \tline B\n\
             {SHA_A} 3 3\n\
             filename file.rs\n\
             \tline A again\n\
             {SHA_C} 4 4 1\n\
             author Carol\n\
             author-mail <carol@example.com>\n\
             author-time 1000000200\n\
             author-tz +0000\n\
             committer Carol\n\
             committer-mail <carol@example.com>\n\
             committer-time 1000000200\n\
             committer-tz +0000\n\
             summary C commit\n\
             filename file.rs\n\
             \tline C\n"
        );
        let lines = parse_blame(&input);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].commit.sha, SHA_A);
        assert_eq!(lines[0].commit.author, "Alice");
        assert_eq!(lines[1].commit.sha, SHA_B);
        assert_eq!(lines[1].commit.author, "Bob");
        assert_eq!(lines[2].commit.sha, SHA_A);
        assert_eq!(lines[2].commit.author, "Alice");
        assert_eq!(lines[2].content, "line A again");
        assert_eq!(lines[3].commit.sha, SHA_C);
        assert_eq!(lines[3].commit.author, "Carol");
        assert_eq!(lines[3].commit.summary, "C commit");
    }
}
