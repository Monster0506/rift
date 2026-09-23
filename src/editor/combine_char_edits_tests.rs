use super::combine_char_edits;
use crate::buffer::CharEdit;

fn edit(pos: usize, del: usize, ins: usize) -> CharEdit {
    CharEdit { pos, del, ins }
}

#[test]
fn empty_returns_none() {
    assert_eq!(combine_char_edits(&[]), None);
}

#[test]
fn single_edit_passes_through_unchanged() {
    let e = edit(10, 2, 3);
    assert_eq!(combine_char_edits(&[e]), Some(e));
}

#[test]
fn descending_single_char_deletes_combine() {
    // Rightmost-first, as Transaction::inverse() replays an undone insert.
    let edits = [edit(9, 1, 0), edit(8, 1, 0), edit(7, 1, 0)];
    assert_eq!(combine_char_edits(&edits), Some(edit(7, 3, 0)));
}

#[test]
fn fixed_position_single_char_deletes_combine() {
    // Repeated forward-delete at an unmoving cursor.
    let edits = [edit(5, 1, 0), edit(5, 1, 0), edit(5, 1, 0)];
    assert_eq!(combine_char_edits(&edits), Some(edit(5, 3, 0)));
}

#[test]
fn ascending_single_char_inserts_combine() {
    let edits = [edit(4, 0, 1), edit(5, 0, 1), edit(6, 0, 1)];
    assert_eq!(combine_char_edits(&edits), Some(edit(4, 0, 3)));
}

#[test]
fn ascending_deletes_are_rejected() {
    // Not a real observed pattern; would skip characters if combined.
    let edits = [edit(5, 1, 0), edit(6, 1, 0), edit(7, 1, 0)];
    assert_eq!(combine_char_edits(&edits), None);
}

#[test]
fn descending_inserts_are_rejected() {
    let edits = [edit(6, 0, 1), edit(5, 0, 1), edit(4, 0, 1)];
    assert_eq!(combine_char_edits(&edits), None);
}

#[test]
fn delete_then_insert_same_position_combines_as_replace() {
    // A single-char replace as two ops (delete then insert at the same
    // spot) touches one contiguous region, so it should combine.
    let edits = [edit(5, 1, 0), edit(5, 0, 1)];
    assert_eq!(combine_char_edits(&edits), Some(edit(5, 1, 1)));
}

#[test]
fn adjacent_multi_char_edits_of_different_sizes_combine() {
    // The `dd` shape: a bulk line-content delete followed by a separate
    // 1-char newline delete immediately to its left, in current coords.
    let edits = [edit(1202714, 1362, 0), edit(1202713, 1, 0)];
    assert_eq!(combine_char_edits(&edits), Some(edit(1202713, 1363, 0)));
}

#[test]
fn adjacent_multi_char_inserts_of_different_sizes_combine() {
    // Two adjacent insertions restore a single deleted range.
    let edits = [edit(1202713, 0, 1), edit(1202714, 0, 1362)];
    assert_eq!(combine_char_edits(&edits), Some(edit(1202713, 0, 1363)));
}

#[test]
fn multi_char_single_edits_with_a_gap_are_rejected() {
    // [5,8) and [2,4) don't touch - a real 1-char gap at position 4.
    let edits = [edit(5, 3, 0), edit(2, 2, 0)];
    assert_eq!(combine_char_edits(&edits), None);
}

#[test]
fn non_contiguous_deletes_are_rejected() {
    let edits = [edit(9, 1, 0), edit(5, 1, 0)];
    assert_eq!(combine_char_edits(&edits), None);
}

#[test]
fn descending_then_repeated_delete_combines() {
    // Undo of "open a line, then type": descending char-deletes (the
    // typed text reversed) plus one more delete at the same landing spot.
    let edits = [
        edit(3, 1, 0),
        edit(2, 1, 0),
        edit(1, 1, 0),
        edit(0, 1, 0),
        edit(0, 1, 0),
    ];
    assert_eq!(combine_char_edits(&edits), Some(edit(0, edits.len(), 0)));
}

#[test]
fn repeated_then_ascending_insert_combines() {
    // Repeated ascending insertions recreate one transaction.
    let edits = [edit(0, 0, 1), edit(0, 0, 1), edit(1, 0, 1), edit(2, 0, 1)];
    assert_eq!(combine_char_edits(&edits), Some(edit(0, 0, edits.len())));
}

#[test]
fn interleaved_zero_and_descending_deletes_combine() {
    let edits = [
        edit(5, 1, 0),
        edit(5, 1, 0),
        edit(4, 1, 0),
        edit(4, 1, 0),
        edit(3, 1, 0),
    ];
    assert_eq!(combine_char_edits(&edits), Some(edit(3, edits.len(), 0)));
}

#[test]
fn a_jump_larger_than_one_is_rejected() {
    let edits = [edit(9, 1, 0), edit(7, 1, 0)];
    assert_eq!(combine_char_edits(&edits), None);
}
