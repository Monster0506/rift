use super::*;
use crate::buffer::TextBuffer;
use crate::wrap::RangeKind;

fn region(a: usize, c: usize) -> Region {
    Region::new(a, c, RangeKind::Charwise)
}

#[test]
fn touching_regions_do_not_merge() {
    // "foo\n\nfoofoo": bank "foo" (0..2), then the two "foo"s inside "foofoo"
    // at indices 6..8 and 9..11 -- 8's neighbor is 9, sharing zero chars.
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(6, 8));
    set.bank(region(9, 11));
    assert_eq!(set.regions.len(), 3, "touching regions must stay independent");
}

#[test]
fn true_overlap_merges_into_union() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 5));
    set.bank(region(3, 8));
    assert_eq!(set.regions.len(), 1);
    assert_eq!(set.regions[0].span(), (0, 9));
}

#[test]
fn overlap_merge_chains_through_a_third_region() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(10, 12));
    // Overlaps both the first (0..2) and the second (10..12) at once.
    set.bank(region(1, 11));
    assert_eq!(set.regions.len(), 1);
    assert_eq!(set.regions[0].span(), (0, 13));
}

#[test]
fn merge_never_crosses_kinds() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 5));
    set.bank(Region::new(2, 7, RangeKind::Linewise));
    assert_eq!(set.regions.len(), 2, "Charwise and Linewise must never merge");
}

#[test]
fn commit_active_banks_into_the_set() {
    let mut set = SelectionSet::default();
    set.active = Some(region(0, 3));
    set.commit_active();
    assert!(set.active.is_none());
    assert_eq!(set.regions.len(), 1);
}

#[test]
fn take_for_batch_orders_highest_offset_first_and_clears() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(10, 12));
    set.bank(region(20, 22));
    let batch = set.take_for_batch();
    assert_eq!(
        batch.iter().map(|r| r.span().0).collect::<Vec<_>>(),
        vec![20, 10, 0]
    );
    assert!(set.is_empty());
}

#[test]
fn next_region_cycles_and_wraps() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(10, 12));
    assert_eq!(set.next_region(5).unwrap().span().0, 10);
    assert_eq!(set.next_region(15).unwrap().span().0, 0, "wraps to first");
}

#[test]
fn prev_region_cycles_and_wraps() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(10, 12));
    assert_eq!(set.prev_region(15).unwrap().span().0, 10);
    assert_eq!(set.prev_region(1).unwrap().span().0, 10, "wraps to last");
}

#[test]
fn bank_occurrence_finds_next_match_and_wraps() {
    let mut buf = TextBuffer::new(20).unwrap();
    buf.insert_str("foo bar foo baz foo").unwrap();
    let mut set = SelectionSet::default();
    set.bank(region(0, 2)); // first "foo"
    let (found, needle) = set.bank_occurrence(&buf, true).unwrap();
    assert_eq!(needle, "foo");
    assert_eq!(found.span(), (8, 11));
    assert_eq!(set.regions.len(), 2);
}

#[test]
fn bank_occurrence_returns_none_when_all_occurrences_banked() {
    let mut buf = TextBuffer::new(20).unwrap();
    buf.insert_str("foo foo").unwrap();
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(4, 6));
    assert!(set.bank_occurrence(&buf, true).is_none());
}

#[test]
fn bank_occurrence_disabled_for_blockwise() {
    let mut buf = TextBuffer::new(20).unwrap();
    buf.insert_str("foo foo").unwrap();
    let mut set = SelectionSet::default();
    set.bank(Region::new(0, 2, RangeKind::Blockwise));
    assert!(set.bank_occurrence(&buf, true).is_none());
}

#[test]
fn bank_occurrence_matches_the_whole_line_for_linewise_not_one_char() {
    let mut buf = TextBuffer::new(20).unwrap();
    buf.insert_str("foo\nbar\nfoo\n").unwrap();
    let mut set = SelectionSet::default();
    // V at the line start with no movement: anchor == cursor == 0. Using
    // span() instead of buffer_span() would collapse the needle to "f".
    set.bank(Region::new(0, 0, RangeKind::Linewise));
    let (found, needle) = set.bank_occurrence(&buf, true).unwrap();
    assert_eq!(needle, "foo\n");
    assert_eq!(found.buffer_span(&buf), (8, 12));
}

#[test]
fn blockwise_regions_only_merge_with_other_blockwise_overlap() {
    let mut set = SelectionSet::default();
    set.bank(Region::new(0, 5, RangeKind::Blockwise));
    set.bank(Region::new(3, 8, RangeKind::Charwise)); // overlaps in raw offsets, wrong kind
    assert_eq!(set.regions.len(), 2, "Blockwise must not merge with an overlapping Charwise region");

    set.bank(Region::new(3, 8, RangeKind::Blockwise)); // overlaps the first Blockwise region
    assert_eq!(set.regions.len(), 2, "but DOES merge with an overlapping Blockwise region");
}
