use super::*;

fn buf_from(s: &str) -> TextBuffer {
    let mut b = TextBuffer::new(s.len().max(16)).unwrap();
    b.insert_str(s).unwrap();
    b
}

fn insert_at(buf: &mut TextBuffer, pos: usize, s: &str) -> (usize, usize, usize) {
    buf.set_cursor(pos).unwrap();
    buf.insert_str(s).unwrap();
    (pos, 0, s.chars().count())
}

fn delete_at(buf: &mut TextBuffer, pos: usize, n: usize) -> (usize, usize, usize) {
    assert!(buf.delete_range(pos, n));
    (pos, n, 0)
}

fn check_edit(
    text: &str,
    wrap_width: usize,
    edit: impl FnOnce(&mut TextBuffer) -> (usize, usize, usize),
) {
    let mut buf = buf_from(text);
    let mut map = DisplayMap::build(&buf, wrap_width, 4);
    let (pos, del, ins) = edit(&mut buf);
    assert!(
        map.apply_edit(&buf, pos, del, ins, Vec::new()),
        "apply_edit refused"
    );
    let full = DisplayMap::build(&buf, wrap_width, 4);
    assert_eq!(map, full, "incremental map diverged from full rebuild");
}

const SAMPLE: &str = "hello world foo bar\nsecond line right here\nthird\n";

#[test]
fn apply_edit_insert_char_mid_line() {
    check_edit(SAMPLE, 8, |b| insert_at(b, 7, "XY"));
}

#[test]
fn apply_edit_insert_newline_splits_line() {
    check_edit(SAMPLE, 8, |b| insert_at(b, 5, "\n"));
}

#[test]
fn apply_edit_insert_multiline() {
    check_edit(SAMPLE, 8, |b| insert_at(b, 3, "one\ntwo\nthree"));
}

#[test]
fn apply_edit_delete_char() {
    check_edit(SAMPLE, 8, |b| delete_at(b, 6, 1));
}

#[test]
fn apply_edit_delete_newline_joins_lines() {
    let nl = SAMPLE.find('\n').unwrap();
    check_edit(SAMPLE, 8, |b| delete_at(b, nl, 1));
}

#[test]
fn exactly_full_line_gets_an_eol_row_only_when_asked() {
    // "abcdefgh" is exactly 8 wide; "ab" is not.
    let buf = buf_from("abcdefgh\nab\nabcdefgh");
    let plain = DisplayMap::build(&buf, 8, 4);
    assert_eq!(plain.total_visual_rows(), 3, "no extra row by default");

    let with = DisplayMap::build_with(&buf, 8, 4, vec![0, 1]);
    assert_eq!(with.total_visual_rows(), 4);
    let eol = with.get_visual_row(1).unwrap();
    assert_eq!((eol.char_start, eol.char_end, eol.is_first), (8, 8, false));
    assert_eq!(with.char_to_visual_row(7), 0);
    assert_eq!(
        with.char_to_visual_row(8),
        1,
        "the EOL position lives on its own row"
    );
    // Line 1 is short: asking for an EOL row changes nothing there.
    assert_eq!(with.logical_to_first_visual(1), 2);
    assert_eq!(with.logical_to_first_visual(2), 3);

    // Vertical motion steps over the EOL-only row in both directions.
    assert_eq!(with.visual_down(3, &buf), 9 + 2);
    assert_eq!(with.visual_up(9, &buf), 0);
}

#[test]
fn apply_edit_keeps_eol_rows_in_step_with_a_full_rebuild() {
    let text = "abcdefgh\nsecond line here\nabcdefgh\n";
    let mut buf = buf_from(text);
    let mut map = DisplayMap::build_with(&buf, 8, 4, vec![0, 2]);

    // Splitting line 1 shifts the second adorned line to 3.
    let (pos, del, ins) = insert_at(&mut buf, 12, "\n");
    assert!(map.apply_edit(&buf, pos, del, ins, vec![0, 3]));
    assert_eq!(map, DisplayMap::build_with(&buf, 8, 4, vec![0, 3]));

    // A set that disagrees outside the edited region forces a rebuild.
    let (pos, del, ins) = insert_at(&mut buf, 12, "x");
    assert!(!map.apply_edit(&buf, pos, del, ins, vec![3]));
}

#[test]
fn apply_edit_delete_across_lines() {
    check_edit(SAMPLE, 8, |b| delete_at(b, 15, 12));
}

#[test]
fn apply_edit_at_buffer_start_and_end() {
    check_edit(SAMPLE, 8, |b| insert_at(b, 0, "zz "));
    let len = SAMPLE.chars().count();
    check_edit(SAMPLE, 8, |b| insert_at(b, len, "tail"));
}

#[test]
fn apply_edit_on_last_line_without_trailing_newline() {
    check_edit("first line\nlast has no newline", 8, |b| {
        insert_at(b, 15, "wrap me please")
    });
    check_edit("first line\nlast has no newline", 8, |b| {
        delete_at(b, 12, 6)
    });
}

#[test]
fn apply_edit_delete_everything() {
    let len = SAMPLE.chars().count();
    check_edit(SAMPLE, 8, |b| delete_at(b, 0, len));
}

#[test]
fn apply_edit_insert_into_empty_buffer() {
    check_edit("", 8, |b| insert_at(b, 0, "abc def ghi"));
    check_edit("", 8, |b| insert_at(b, 0, "two\nlines"));
}

#[test]
fn apply_edit_char_wraps_long_word() {
    check_edit("abcdefghijklmnopqrstuvwxyz", 8, |b| {
        insert_at(b, 13, "0123")
    });
}

#[test]
fn apply_edit_with_tabs_and_wide_chars() {
    check_edit("a\tb\tc d e f\n", 8, |b| insert_at(b, 3, "\tx"));
    check_edit("你好世界 hello there\n", 6, |b| insert_at(b, 2, "x"));
    check_edit("你好世界 hello there\n", 6, |b| delete_at(b, 1, 3));
}

#[test]
fn apply_edit_replace_range() {
    check_edit(SAMPLE, 8, |b| {
        let chars: Vec<crate::character::Character> = "REPLACED".chars().map(Into::into).collect();
        assert!(b.replace_range(4, 9, &chars));
        (4, 9, chars.len())
    });
}

#[test]
fn apply_edit_rejects_mismatched_buffer() {
    let buf = buf_from(SAMPLE);
    let mut map = DisplayMap::build(&buf, 8, 4);
    let other = buf_from("completely different text of another length");
    assert!(!map.apply_edit(&other, 0, 0, 1, Vec::new()));
}

#[test]
fn apply_edit_matches_full_rebuild_random() {
    fn next(seed: &mut u64, m: usize) -> usize {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 33) as usize) % m.max(1)
    }

    let mut seed: u64 = 0x8765_4321;
    let inserts = ["x", "hello ", "\n", "a\nb", "  ", "wrapping-word", "\t"];
    let mut buf = buf_from(&"the quick brown fox jumps over the lazy dog\n".repeat(8));
    let mut map = DisplayMap::build(&buf, 10, 4);

    for i in 0..300 {
        let len = buf.len();
        let (pos, del, ins) = if next(&mut seed, 2) == 0 || len == 0 {
            let s = inserts[next(&mut seed, inserts.len())];
            insert_at(&mut buf, next(&mut seed, len + 1), s)
        } else {
            let pos = next(&mut seed, len);
            let n = (next(&mut seed, 8) + 1).min(len - pos);
            delete_at(&mut buf, pos, n)
        };
        assert!(
            map.apply_edit(&buf, pos, del, ins, Vec::new()),
            "apply_edit refused at step {i}"
        );
        let full = DisplayMap::build(&buf, 10, 4);
        assert_eq!(map, full, "diverged at step {i}");
    }
}

/// A rightmost-first single-char delete run: collapsing it into one net
/// `apply_edit` call must match applying it incrementally and a full rebuild.
#[test]
fn apply_edit_combined_delete_matches_full_rebuild() {
    let mut buf = buf_from(SAMPLE);
    let mut map = DisplayMap::build(&buf, 8, 4);
    let (pos, del, ins) = insert_at(&mut buf, 6, "ABCDE");
    assert!(map.apply_edit(&buf, pos, del, ins, Vec::new()));

    for p in (6..11).rev() {
        assert!(buf.delete_range(p, 1));
    }
    assert!(
        map.apply_edit(&buf, 6, 5, 0, Vec::new()),
        "combined delete apply_edit refused"
    );
    let full = DisplayMap::build(&buf, 8, 4);
    assert_eq!(map, full, "combined delete diverged from full rebuild");
}

/// A single-char delete run at a fixed position, mirroring repeated
/// forward-delete (`<Del>`) at an unmoving cursor.
#[test]
fn apply_edit_combined_fixed_position_delete_matches_full_rebuild() {
    let mut buf = buf_from(SAMPLE);
    let mut map = DisplayMap::build(&buf, 8, 4);
    let (pos, del, ins) = insert_at(&mut buf, 6, "ABCDE");
    assert!(map.apply_edit(&buf, pos, del, ins, Vec::new()));

    for _ in 0..5 {
        assert!(buf.delete_range(6, 1));
    }
    assert!(
        map.apply_edit(&buf, 6, 5, 0, Vec::new()),
        "combined fixed-position delete apply_edit refused"
    );
    let full = DisplayMap::build(&buf, 8, 4);
    assert_eq!(map, full, "combined delete diverged from full rebuild");
}

/// An ascending single-char insert run, mirroring forward typing or a
/// redone multi-char insert, one character at a time.
#[test]
fn apply_edit_combined_insert_matches_full_rebuild() {
    let mut buf = buf_from(SAMPLE);
    let mut map = DisplayMap::build(&buf, 8, 4);

    let start = 6;
    for (i, ch) in "ABCDE".chars().enumerate() {
        insert_at(&mut buf, start + i, &ch.to_string());
    }
    assert!(
        map.apply_edit(&buf, start, 0, 5, Vec::new()),
        "combined insert apply_edit refused"
    );
    let full = DisplayMap::build(&buf, 8, 4);
    assert_eq!(map, full, "combined insert diverged from full rebuild");
}

/// Undo of "open a line, then type": N descending char-deletes (typed
/// text reversed) plus one more delete landing back at the newline's spot.
#[test]
fn apply_edit_combined_descending_then_repeat_matches_full_rebuild() {
    let mut buf = buf_from(SAMPLE);
    let mut map = DisplayMap::build(&buf, 8, 4);

    let (pos, del, ins) = insert_at(&mut buf, 6, "\n");
    assert!(map.apply_edit(&buf, pos, del, ins, Vec::new()));
    for (i, ch) in "ABCDE".chars().enumerate() {
        let (pos, del, ins) = insert_at(&mut buf, 7 + i, &ch.to_string());
        assert!(map.apply_edit(&buf, pos, del, ins, Vec::new()));
    }

    for p in (7..12).rev() {
        assert!(buf.delete_range(p, 1));
    }
    assert!(buf.delete_range(6, 1));
    assert!(
        map.apply_edit(&buf, 6, 6, 0, Vec::new()),
        "combined descending-then-repeat apply_edit refused"
    );
    let full = DisplayMap::build(&buf, 8, 4);
    assert_eq!(
        map, full,
        "combined descending-then-repeat diverged from full rebuild"
    );
}

/// The `dd` shape (bulk line delete + adjacent newline delete), combined
/// via the real `combine_char_edits`, not a hand-computed triple.
#[test]
fn combine_char_edits_handles_dd_shaped_batch() {
    use crate::buffer::CharEdit;
    use crate::editor::rendering::combine_char_edits;

    let text = format!("{}line to delete\nnext\n", "pad ".repeat(20));
    let mut buf = buf_from(&text);
    let mut map = DisplayMap::build(&buf, 12, 4);

    let line_start = text.find("line to delete").unwrap();
    let line_len = "line to delete\n".len();
    assert!(buf.delete_range(line_start, line_len - 1));
    assert!(buf.delete_range(line_start, 1));
    let edits = [
        CharEdit {
            pos: line_start,
            del: line_len - 1,
            ins: 0,
        },
        CharEdit {
            pos: line_start,
            del: 1,
            ins: 0,
        },
    ];

    let combined = combine_char_edits(&edits).expect("dd shape should combine");
    assert!(map.apply_edit(&buf, combined.pos, combined.del, combined.ins, Vec::new()));
    let full = DisplayMap::build(&buf, 12, 4);
    assert_eq!(
        map, full,
        "dd-shaped combined edit diverged from full rebuild"
    );
}

/// Randomized batches of 2-6 edits near a moving anchor: every batch
/// `combine_char_edits` accepts must match a from-scratch rebuild.
#[test]
fn combine_char_edits_matches_full_rebuild_across_random_batches() {
    use crate::buffer::CharEdit;
    use crate::editor::rendering::combine_char_edits;

    fn next(seed: &mut u64, m: usize) -> usize {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 33) as usize) % m.max(1)
    }

    let mut seed: u64 = 0x1357_9bdf_2468_ace0;
    let inserts = ["x", "hi", "\n", "wrap-me", "  ", "a\nb"];

    for trial in 0..300 {
        let mut buf = buf_from(&"the quick brown fox jumps over the lazy dog\n".repeat(6));
        let mut map = DisplayMap::build(&buf, 10, 4);

        let n_edits = 2 + next(&mut seed, 5); // 2..=6
        let mut edits: Vec<CharEdit> = Vec::new();
        let mut anchor = next(&mut seed, buf.len().max(1));

        for _ in 0..n_edits {
            let len = buf.len();
            if len == 0 {
                break;
            }
            let gap = if next(&mut seed, 4) == 0 {
                next(&mut seed, 5) + 1
            } else {
                0
            };
            let pos = (anchor + gap).min(len);
            let (p, d, i) = if next(&mut seed, 2) == 0 {
                let s = inserts[next(&mut seed, inserts.len())];
                insert_at(&mut buf, pos, s)
            } else {
                let n = (next(&mut seed, 5) + 1).min(len - pos);
                if n == 0 {
                    continue;
                }
                delete_at(&mut buf, pos, n)
            };
            edits.push(CharEdit {
                pos: p,
                del: d,
                ins: i,
            });
            anchor = p + i;
        }
        if edits.is_empty() {
            continue;
        }

        if let Some(combined) = combine_char_edits(&edits) {
            assert!(
                map.apply_edit(&buf, combined.pos, combined.del, combined.ins, Vec::new()),
                "trial {trial}: apply_edit refused a combined edit"
            );
            let full = DisplayMap::build(&buf, 10, 4);
            assert_eq!(
                map, full,
                "trial {trial}: combined-edit map diverged from full rebuild (edits={edits:?})"
            );
        }
        // None means a non-contiguous batch was correctly detected - the
        // safe fallback-to-full-rebuild path, nothing to check here.
    }
}

fn lcg_next(seed: &mut u64, m: usize) -> usize {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 33) as usize) % m.max(1)
}

/// Drives a lazily-extended `DisplayMap` through randomized out-of-order
/// queries, asserting every answer matches a full eager `DisplayMap::build`.
fn assert_lazy_matches_full(text: &str, wrap_width: usize, tab_width: usize, seed: u64) {
    let buf = buf_from(text);
    let full = DisplayMap::build(&buf, wrap_width, tab_width);
    let mut lazy = DisplayMap::empty(wrap_width, tab_width);
    let len = buf.len();

    let mut seed = seed;
    let mut offsets: Vec<usize> = Vec::new();
    offsets.push(0);
    if len > 0 {
        offsets.push(len - 1);
        offsets.push(len);
    }
    for _ in 0..60 {
        offsets.push(lcg_next(&mut seed, len + 1));
    }

    for &off in &offsets {
        assert_eq!(
            lazy.char_to_visual_row_ext(off, &buf),
            full.char_to_visual_row(off),
            "char_to_visual_row diverged at offset {off} (text={text:?}, width={wrap_width})"
        );
        assert_eq!(
            lazy.char_to_visual_col_ext(off, &buf),
            full.char_to_visual_col(off, &buf),
            "char_to_visual_col diverged at offset {off} (text={text:?}, width={wrap_width})"
        );
        assert_eq!(
            lazy.visual_down_ext(off, &buf),
            full.visual_down(off, &buf),
            "visual_down diverged at offset {off} (text={text:?}, width={wrap_width})"
        );
        assert_eq!(
            lazy.visual_up_ext(off, &buf),
            full.visual_up(off, &buf),
            "visual_up diverged at offset {off} (text={text:?}, width={wrap_width})"
        );
        assert_eq!(
            lazy.visual_down_to_col_ext(off, 3, &buf),
            full.visual_down_to_col(off, 3, &buf),
            "visual_down_to_col diverged at offset {off} (text={text:?}, width={wrap_width})"
        );
        assert_eq!(
            lazy.visual_up_to_col_ext(off, usize::MAX, &buf),
            full.visual_up_to_col(off, usize::MAX, &buf),
            "visual_up_to_col diverged at offset {off} (text={text:?}, width={wrap_width})"
        );
    }

    let full_rows = full.total_visual_rows();
    for _ in 0..30 {
        let vr = lcg_next(&mut seed, full_rows + 2);
        assert_eq!(
            lazy.get_visual_row_ext(vr, &buf).cloned(),
            full.get_visual_row(vr).cloned(),
            "get_visual_row diverged at row {vr} (text={text:?}, width={wrap_width})"
        );
    }

    lazy.extend_to_end(&buf);
    assert_eq!(
        lazy, full,
        "fully-extended lazy map must equal a fresh full build (text={text:?}, width={wrap_width})"
    );
    assert!(lazy.is_complete());
    assert_eq!(lazy.total_visual_rows(), full.total_visual_rows());
}

#[test]
fn lazy_extension_matches_full_build_basic_sample() {
    assert_lazy_matches_full(SAMPLE, 8, 4, 0x1111_2222);
}

#[test]
fn lazy_extension_matches_full_build_empty_document() {
    assert_lazy_matches_full("", 10, 4, 0x2222_3333);
}

#[test]
fn lazy_extension_matches_full_build_blank_lines_only() {
    assert_lazy_matches_full("\n\n\n\n\n", 10, 4, 0x3333_4444);
}

#[test]
fn lazy_extension_matches_full_build_no_trailing_newline() {
    assert_lazy_matches_full("first line\nlast has no newline", 8, 4, 0x4444_5555);
}

#[test]
fn lazy_extension_matches_full_build_single_huge_word() {
    let text = "x".repeat(500);
    assert_lazy_matches_full(&text, 8, 4, 0x5555_6666);
}

#[test]
fn lazy_extension_matches_full_build_multibyte_utf8() {
    assert_lazy_matches_full(
        "你好世界 hello there\nmore unicode: 日本語のテキストです\n\u{1F600}\u{1F601}\n",
        6,
        4,
        0x6666_7777,
    );
}

#[test]
fn lazy_extension_matches_full_build_tabs_mixed_widths() {
    assert_lazy_matches_full(
        "a\tb\tc\td\te\tf\tg\n\ttabbed start\nend",
        8,
        4,
        0x7777_8888,
    );
}

#[test]
fn lazy_extension_matches_full_build_crosses_extend_batch_boundary() {
    // EXTEND_BATCH_LINES is 256; use enough lines that extension spans
    // multiple internal batches, exercising the extend loop itself.
    let text = "the quick brown fox jumps over the lazy dog\n".repeat(600);
    assert_lazy_matches_full(&text, 10, 4, 0x8888_9999);
}

#[test]
fn lazy_extension_matches_full_build_varied_widths() {
    let text =
        "the quick brown fox jumps over the lazy dog\nsecond line here\n\nfourth\n".repeat(20);
    for width in [1usize, 2, 3, 5, 8, 20, 100] {
        assert_lazy_matches_full(&text, width, 4, 0x9999_aaaa + width as u64);
    }
}

#[test]
fn needs_extension_reflects_whether_extension_would_do_work() {
    let buf = buf_from(&"line of text here\n".repeat(500));
    let mut dm = DisplayMap::empty(10, 4);
    // Nothing built yet: any real request needs extension.
    assert!(dm.needs_extension(0, 5));
    dm.extend_to_char(&buf, 0);
    dm.extend_to_row(&buf, 5);
    assert!(
        !dm.needs_extension(0, 4),
        "already covers offset 0 + margin 4"
    );
    assert!(
        dm.needs_extension(buf.len() - 1, 0),
        "far offset not yet covered"
    );
    dm.extend_to_end(&buf);
    assert!(
        !dm.needs_extension(buf.len().saturating_sub(1), 1_000_000),
        "complete map never needs extension"
    );
}

#[test]
fn lazy_map_starts_empty_and_incomplete() {
    let dm = DisplayMap::empty(10, 4);
    assert!(!dm.is_complete());
    assert_eq!(dm.total_visual_rows(), 0);
}

#[test]
fn extend_to_row_stops_at_document_end_rather_than_looping() {
    let buf = buf_from("only one short line\n");
    let mut dm = DisplayMap::empty(80, 4);
    dm.extend_to_row(&buf, 1_000_000);
    assert!(dm.is_complete());
    let full = DisplayMap::build(&buf, 80, 4);
    assert_eq!(dm, full);
}

#[test]
fn test_range_kind_blockwise_variant_exists() {
    let _blockwise = RangeKind::Blockwise;
    assert_eq!(_blockwise, RangeKind::Blockwise);
}

#[test]
fn test_motion_range_with_blockwise() {
    let range = MotionRange {
        anchor: 10,
        new_cursor: 20,
        kind: RangeKind::Blockwise,
        inclusive: false,
    };
    assert_eq!(range.kind, RangeKind::Blockwise);
    assert_eq!(range.anchor, 10);
    assert_eq!(range.new_cursor, 20);
    assert!(!range.inclusive);
}
