use super::*;

/// Helper: apply a pure insert of `len` bytes at `at`.
fn insert(m: &mut Marker, at: usize, len: usize) {
    m.on_edit(at, at, at + len);
}

/// Helper: apply a pure delete of `[start, end)`.
fn delete(m: &mut Marker, start: usize, end: usize) {
    m.on_edit(start, end, start);
}

#[test]
fn insert_before_marker_shifts_right() {
    let mut m = Marker::left(10);
    insert(&mut m, 3, 2);
    assert_eq!(m.offset, 12);
}

#[test]
fn insert_after_marker_unchanged() {
    let mut m = Marker::right(10);
    insert(&mut m, 20, 5);
    assert_eq!(m.offset, 10);
}

#[test]
fn insert_at_marker_left_gravity_stays() {
    let mut m = Marker::left(10);
    insert(&mut m, 10, 4);
    assert_eq!(
        m.offset, 10,
        "left gravity: inserted text lands to the right"
    );
}

#[test]
fn insert_at_marker_right_gravity_moves() {
    let mut m = Marker::right(10);
    insert(&mut m, 10, 4);
    assert_eq!(m.offset, 14, "right gravity: marker pushed right");
}

#[test]
fn delete_fully_before_shifts_left() {
    let mut m = Marker::left(10);
    delete(&mut m, 2, 5); // delete 3 bytes entirely before the marker
    assert_eq!(m.offset, 7);
}

#[test]
fn delete_spanning_marker_clamps_to_start() {
    let mut m = Marker::left(8);
    delete(&mut m, 5, 12); // marker at 8 is inside [5,12)
    assert_eq!(m.offset, 5);
}

#[test]
fn delete_at_left_boundary_keeps_marker() {
    let mut m = Marker::left(5);
    delete(&mut m, 5, 9); // deletion starts exactly at the marker
    assert_eq!(m.offset, 5);
}

#[test]
fn delete_ending_at_marker_pulls_to_start() {
    let mut m = Marker::left(9);
    delete(&mut m, 5, 9); // deletion ends exactly at the marker
    assert_eq!(m.offset, 5);
}

#[test]
fn replace_inside_applies_net_delta_with_clamp() {
    // Replace [4,11) (7 bytes) with 3 bytes. A marker after the region shifts by -4.
    let mut after = Marker::right(15);
    after.on_edit(4, 11, 4 + 3);
    assert_eq!(after.offset, 11);

    // A marker inside the replaced region collapses to start.
    let mut inside = Marker::left(8);
    inside.on_edit(4, 11, 4 + 3);
    assert_eq!(inside.offset, 4);
}

#[test]
fn typical_range_extends_when_typing_inside() {
    // start: left-gravity, end: right-gravity over [4, 11).
    let mut start = Marker::left(4);
    let mut end = Marker::right(11);
    // Type 2 chars at offset 7 (inside the range).
    insert(&mut start, 7, 2);
    insert(&mut end, 7, 2);
    assert_eq!(start.offset, 4, "start unaffected by interior insert");
    assert_eq!(end.offset, 13, "end extends by the inserted length");
}

#[test]
fn typing_just_outside_does_not_extend() {
    let mut start = Marker::left(4);
    let mut end = Marker::right(11);
    // Type exactly at the end boundary 11. Right-gravity end moves; this is the
    // "text typed inside extends it" boundary behavior.
    insert(&mut start, 0, 1); // before: shifts both
    insert(&mut end, 0, 1);
    assert_eq!(start.offset, 5);
    assert_eq!(end.offset, 12);
    // Now type at the left boundary (5); left-gravity start should not move.
    insert(&mut start, 5, 3);
    assert_eq!(
        start.offset, 5,
        "left-gravity start does not absorb a boundary insert"
    );
}
