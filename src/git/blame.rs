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
#[path = "blame_tests.rs"]
mod blame_tests;
