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
#[path = "log_tests.rs"]
mod log_tests;
