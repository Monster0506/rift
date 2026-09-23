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
