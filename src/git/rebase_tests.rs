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
