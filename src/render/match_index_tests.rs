use super::match_index_at_cursor;
use crate::search::SearchMatch;

fn make_matches(ranges: &[(usize, usize)]) -> Vec<SearchMatch> {
    ranges
        .iter()
        .map(|&(s, e)| SearchMatch { range: s..e })
        .collect()
}

#[test]
fn finds_index_of_match_containing_cursor_in_large_sorted_list() {
    let ranges: Vec<(usize, usize)> = (0..10_000).map(|i| (i * 10, i * 10 + 3)).collect();
    let matches = make_matches(&ranges);

    assert_eq!(match_index_at_cursor(&matches, 0), Some(1));
    assert_eq!(match_index_at_cursor(&matches, 5_001), Some(501));
    assert_eq!(match_index_at_cursor(&matches, 50_010), Some(5_002));
    assert_eq!(match_index_at_cursor(&matches, 99_990 + 2), Some(10_000));
}

#[test]
fn returns_none_when_cursor_is_between_matches() {
    let matches = make_matches(&[(0, 3), (10, 13)]);
    assert_eq!(match_index_at_cursor(&matches, 5), None);
}

#[test]
fn matches_zero_width_match_at_cursor_start() {
    let matches = make_matches(&[(0, 0), (5, 5)]);
    assert_eq!(match_index_at_cursor(&matches, 5), Some(2));
}

#[test]
fn returns_none_for_empty_matches() {
    let matches: Vec<SearchMatch> = Vec::new();
    assert_eq!(match_index_at_cursor(&matches, 0), None);
}
