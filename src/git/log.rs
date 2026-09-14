//! Parses the custom-delimited log format. Explicit delimiters keep multi-line commit bodies intact.

/// The `--format=...` string [`parse_log`] expects its input to have been produced with. Callers building the `git log` subprocess invocation MUST use exactly this format string: full hash, abbreviated hash, author name, author email, author unix timestamp, space-separated parent hashes, subject, body.
pub const LOG_FORMAT: &str = "%H%x1f%h%x1f%an%x1f%ae%x1f%at%x1f%P%x1f%s%x1f%b%x1e";

/// One commit's worth of `git log` summary data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub sha: String,
    pub short_sha: String,
    pub author_name: String,
    pub author_email: String,
    pub author_time: i64,
    pub subject: String,
    /// May be empty. Never includes the trailing record separator, and; unlike the record's first field; its own leading newline (if any) is preserved verbatim: a blank first line of a body is meaningful, unlike the single incidental newline git inserts before `%H`.
    pub body: String,
    pub parents: Vec<String>,
}

/// Parse the full stdout of `git log --format=<LOG_FORMAT>`. A chunk that doesn't split into exactly the 8 fields `LOG_FORMAT` produces; an empty trailing chunk after the final record separator, or a stream truncated mid-record; is skipped rather than panicking or corrupting the commits already parsed.
pub fn parse_log(input: &str) -> Vec<CommitSummary> {
    let mut result = Vec::new();

    for chunk in input.split('\u{1e}') {
        if chunk.is_empty() {
            continue;
        }
        let fields: Vec<&str> = chunk.split('\u{1f}').collect();
        if fields.len() != 8 {
            continue;
        }

        // Git inserts one incidental newline between the previous record's `%x1e` and this record's `%H`; strip exactly that one, but only from the hash field; never from `body`, where a leading newline would be an intentional blank first line.
        let sha = fields[0]
            .strip_prefix('\n')
            .unwrap_or(fields[0])
            .to_string();
        let short_sha = fields[1].to_string();
        let author_name = fields[2].to_string();
        let author_email = fields[3].to_string();
        let author_time: i64 = fields[4].trim().parse().unwrap_or(0);
        let parents = fields[5]
            .split_ascii_whitespace()
            .map(str::to_string)
            .collect();
        let subject = fields[6].to_string();
        let body = fields[7].to_string();

        result.push(CommitSummary {
            sha,
            short_sha,
            author_name,
            author_email,
            author_time,
            subject,
            body,
            parents,
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Join fields with the unit separator and terminate with the record
    /// separator, mirroring exactly what `LOG_FORMAT` produces per commit.
    fn record(fields: &[&str]) -> String {
        format!("{}\x1e", fields.join("\x1f"))
    }

    #[test]
    fn parses_single_commit_with_empty_body() {
        let input = record(&[
            "hash1",
            "h1",
            "Alice",
            "alice@example.com",
            "1000000000",
            "",
            "Subject line",
            "",
        ]);
        let commits = parse_log(&input);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, "hash1");
        assert_eq!(commits[0].short_sha, "h1");
        assert_eq!(commits[0].author_name, "Alice");
        assert_eq!(commits[0].author_email, "alice@example.com");
        assert_eq!(commits[0].author_time, 1_000_000_000);
        assert_eq!(commits[0].subject, "Subject line");
        assert_eq!(commits[0].body, "");
        assert_eq!(commits[0].parents, Vec::<String>::new());
    }

    #[test]
    fn parses_multiline_body_verbatim() {
        let body = "Line one.\n\nLine three after a blank line.\nLine four.";
        let input = record(&[
            "hash1",
            "h1",
            "Alice",
            "alice@example.com",
            "1000000000",
            "parent1",
            "Subject",
            body,
        ]);
        let commits = parse_log(&input);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].body, body);
    }

    #[test]
    fn parses_multiple_commits_in_order() {
        let mut input = record(&[
            "hash1",
            "h1",
            "Alice",
            "alice@example.com",
            "1000000000",
            "",
            "First",
            "",
        ]);
        input.push_str(&record(&[
            "hash2",
            "h2",
            "Bob",
            "bob@example.com",
            "1000000100",
            "hash1",
            "Second",
            "",
        ]));
        input.push_str(&record(&[
            "hash3",
            "h3",
            "Carol",
            "carol@example.com",
            "1000000200",
            "hash2",
            "Third",
            "",
        ]));

        let commits = parse_log(&input);
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].subject, "First");
        assert_eq!(commits[1].subject, "Second");
        assert_eq!(commits[2].subject, "Third");
    }

    #[test]
    fn merge_commit_parses_two_parents() {
        let input = record(&[
            "merge1",
            "m1",
            "Alice",
            "alice@example.com",
            "1000000000",
            "parent1 parent2",
            "Merge branch 'foo'",
            "",
        ]);
        let commits = parse_log(&input);
        assert_eq!(commits.len(), 1);
        assert_eq!(
            commits[0].parents,
            vec!["parent1".to_string(), "parent2".to_string()]
        );
    }

    #[test]
    fn root_commit_has_no_parents() {
        let input = record(&[
            "root1",
            "r1",
            "Alice",
            "alice@example.com",
            "1000000000",
            "",
            "Initial commit",
            "",
        ]);
        let commits = parse_log(&input);
        assert_eq!(commits.len(), 1);
        assert!(commits[0].parents.is_empty());
    }

    #[test]
    fn truncated_trailing_chunk_is_skipped_without_corrupting_earlier_commits() {
        let mut input = record(&[
            "hash1",
            "h1",
            "Alice",
            "alice@example.com",
            "1000000000",
            "",
            "First",
            "",
        ]);
        // Stream cut off mid-write: no record separator, fewer than 8 fields.
        input.push_str("hash2\x1fh2\x1fBob");

        let commits = parse_log(&input);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, "hash1");
        assert_eq!(commits[0].subject, "First");
    }

    #[test]
    fn empty_input_yields_empty_vec() {
        assert_eq!(parse_log(""), Vec::new());
    }
}
