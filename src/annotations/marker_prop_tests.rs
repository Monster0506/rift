//! Property tests for marker maintenance under random edit sequences.

use super::*;
use proptest::prelude::*;

/// A single random edit, generated relative to a current buffer length.
#[derive(Debug, Clone, Copy)]
enum Edit {
    Insert { at: usize, len: usize },
    Delete { start: usize, end: usize },
}

impl Edit {
    /// (start, old_end, new_end) for `Marker::on_edit`, and the new buffer length.
    fn apply(&self, buf_len: usize) -> (usize, usize, usize, usize) {
        match *self {
            Edit::Insert { at, len } => {
                let at = at.min(buf_len);
                (at, at, at + len, buf_len + len)
            }
            Edit::Delete { start, end } => {
                let start = start.min(buf_len);
                let end = end.min(buf_len).max(start);
                (start, end, start, buf_len - (end - start))
            }
        }
    }
}

fn edit_strategy() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (0usize..200, 1usize..20).prop_map(|(at, len)| Edit::Insert { at, len }),
        (0usize..200, 0usize..200).prop_map(|(a, b)| Edit::Delete {
            start: a.min(b),
            end: a.max(b)
        }),
    ]
}

proptest! {
    #[test]
    fn marker_stays_within_buffer_bounds(
        init_len in 1usize..200,
        start_off in 0usize..200,
        edits in proptest::collection::vec(edit_strategy(), 0..40),
    ) {
        let mut len = init_len;
        let off = start_off.min(len);
        let mut left = Marker::left(off);
        let mut right = Marker::right(off);
        for e in edits {
            let (s, oe, ne, new_len) = e.apply(len);
            left.on_edit(s, oe, ne);
            right.on_edit(s, oe, ne);
            len = new_len;
            prop_assert!(left.offset <= len, "left marker {} past buffer len {}", left.offset, len);
            prop_assert!(right.offset <= len, "right marker {} past buffer len {}", right.offset, len);
        }
    }

    #[test]
    fn right_gravity_never_behind_left_gravity(
        init_len in 1usize..200,
        start_off in 0usize..200,
        edits in proptest::collection::vec(edit_strategy(), 0..40),
    ) {
        // Two markers starting at the same offset: under any edit sequence, the
        // right-gravity one is always >= the left-gravity one.
        let mut len = init_len;
        let off = start_off.min(len);
        let mut left = Marker::left(off);
        let mut right = Marker::right(off);
        for e in edits {
            let (s, oe, ne, new_len) = e.apply(len);
            left.on_edit(s, oe, ne);
            right.on_edit(s, oe, ne);
            len = new_len;
            prop_assert!(right.offset >= left.offset);
        }
    }

    #[test]
    fn insert_strictly_before_shifts_by_len(
        off in 5usize..100,
        at in 0usize..100,
        len in 1usize..20,
    ) {
        // A pure insert strictly before the marker always shifts it by exactly len,
        // regardless of gravity.
        prop_assume!(at < off);
        let mut left = Marker::left(off);
        let mut right = Marker::right(off);
        left.on_edit(at, at, at + len);
        right.on_edit(at, at, at + len);
        prop_assert_eq!(left.offset, off + len);
        prop_assert_eq!(right.offset, off + len);
    }

    #[test]
    fn delete_strictly_before_shifts_left_by_len(
        off in 50usize..150,
        start in 0usize..40,
        del in 1usize..10,
    ) {
        let end = start + del;
        prop_assume!(end <= off);
        let mut m = Marker::right(off);
        m.on_edit(start, end, start);
        prop_assert_eq!(m.offset, off - del);
    }
}
