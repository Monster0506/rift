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
