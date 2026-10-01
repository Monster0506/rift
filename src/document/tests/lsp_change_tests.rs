use super::*;

#[cfg(feature = "lsp")]
#[test]
fn test_lsp_char_offset_handles_astral_and_non_ascii() {
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("a🦀é-end");

    use crate::lsp::protocol::PositionEncoding;
    let enc = PositionEncoding::Utf16;
    assert_eq!(doc.lsp_char_offset_in_line(0, 0, enc), 0);
    assert_eq!(
        doc.lsp_char_offset_in_line(0, 3, enc),
        2,
        "utf16 offset 3 (past the 2-unit crab) must land on code point 2 ('é'), not 3"
    );
    assert_eq!(doc.lsp_char_offset_in_line(0, 7, enc), 6);

    assert_eq!(doc.lsp_position_units_in_line(0, 0, enc), 0);
    assert_eq!(
        doc.lsp_position_units_in_line(0, 2, enc),
        3,
        "code point 2 ('é') must report utf16 offset 3 (after the 2-unit crab)"
    );
    assert_eq!(doc.lsp_position_units_in_line(0, 6, enc), 7);
}

#[cfg(feature = "lsp")]
#[test]
fn test_lsp_position_units_differ_by_negotiated_encoding() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("a🦀é-end");

    assert_eq!(
        doc.lsp_position_units_in_line(0, 3, PositionEncoding::Utf16),
        4
    );
    assert_eq!(
        doc.lsp_position_units_in_line(0, 3, PositionEncoding::Utf8),
        7
    );

    assert_eq!(
        doc.lsp_char_offset_in_line(0, 4, PositionEncoding::Utf16),
        3
    );
    assert_eq!(doc.lsp_char_offset_in_line(0, 7, PositionEncoding::Utf8), 3);
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_for_single_char_insert() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("ac");
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup
    let _ = doc.buffer.set_cursor(1);
    let _ = doc.insert_char('b');

    let (range, text) = doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .expect("a single insert must convert to an incremental change");
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 1);
    assert_eq!(range.end.line, 0);
    assert_eq!(range.end.character, 1, "an insertion's range is zero-width");
    assert_eq!(text, "b");
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_for_single_char_delete() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("abc");
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup
    let _ = doc.buffer.set_cursor(2);
    doc.delete_backward();

    let (range, text) = doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .expect("a single delete must convert to an incremental change");
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 1);
    assert_eq!(range.end.line, 0);
    assert_eq!(
        range.end.character, 2,
        "the removed 'b' is one utf16 unit wide"
    );
    assert_eq!(text, "");
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_positions_a_non_bmp_char_correctly() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("🦀x");
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup
    let _ = doc.buffer.set_cursor(2);
    doc.delete_backward();

    let (range, text) = doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .expect("a single delete must convert to an incremental change");
    assert_eq!(range.start.character, 2, "must land after the 2-unit crab");
    assert_eq!(range.end.character, 3);
    assert_eq!(text, "");
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_combines_chaining_inserts() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("a");
    let _ = doc.insert_str("b");

    let (range, text) = doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .expect("two chaining inserts must combine into one incremental change");
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 0);
    assert_eq!(range.end.line, 0);
    assert_eq!(
        range.end.character, 0,
        "a pure insert's range is zero-width"
    );
    assert_eq!(text, "ab");
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_combines_three_chaining_inserts() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("a");
    let _ = doc.insert_str("b");
    let _ = doc.insert_str("c");

    let (range, text) = doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .expect("three chaining inserts must combine into one incremental change");
    assert_eq!(range.start.character, 0);
    assert_eq!(range.end.character, 0);
    assert_eq!(text, "abc");
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_combines_insert_run_spanning_a_newline() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("x\n");
    let _ = doc.insert_str("y");

    let (range, text) = doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .expect("an insert run crossing a newline must still combine");
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 0);
    assert_eq!(range.end.line, 0);
    assert_eq!(range.end.character, 0);
    assert_eq!(text, "x\ny");
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_falls_back_when_inserts_are_not_adjacent() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("aaaa");
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup

    let _ = doc.buffer.set_cursor(0);
    let _ = doc.insert_char('X'); // now: "Xaaaa", cursor after X
    let _ = doc.buffer.set_cursor(5);
    let _ = doc.insert_char('Y'); // unrelated position, doesn't chain from X's insert

    assert!(
        doc.take_incremental_lsp_changes(PositionEncoding::Utf16)
            .is_none(),
        "non-adjacent inserts must fall back to full sync"
    );
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_falls_back_for_mixed_insert_and_delete() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("ab");
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup

    let _ = doc.insert_char('c'); // "abc"
    doc.delete_backward(); // back to "ab" -- one insert, one delete pending

    assert!(
        doc.take_incremental_lsp_changes(PositionEncoding::Utf16)
            .is_none(),
        "a mixed insert+delete batch must fall back to full sync"
    );
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_falls_back_for_multiple_deletes() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("abc");
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup

    doc.delete_backward();
    doc.delete_backward();

    assert!(
        doc.take_incremental_lsp_changes(PositionEncoding::Utf16)
            .is_none(),
        "multiple deletes must still fall back to full sync"
    );
}

#[cfg(feature = "lsp")]
#[test]
fn test_incremental_lsp_change_falls_back_for_multiline_delete() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(1).unwrap();
    let _ = doc.insert_str("ab\ncd");
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup
    let _ = doc.buffer.set_cursor(0);
    let _ = doc.delete_range(0, 3); // removes "ab\n", spanning a line break

    assert!(
        doc.take_incremental_lsp_changes(PositionEncoding::Utf16)
            .is_none(),
        "a delete spanning a newline must fall back to full sync"
    );
}

#[test]
fn test_undo_redo_keep_current_lsp_diagnostics() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("aaa\nbbb\nccc").unwrap();
    let user = doc.annotations.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::range(0, 3),
        AnnotationOwner::User,
    ));
    doc.annotations
        .replace_lsp_diagnostics(vec![(0, None, 1, "old")]);

    doc.delete_range(0, 4).unwrap();
    doc.annotations
        .replace_lsp_diagnostics(vec![(1, None, 2, "new")]);
    let diag_lines = |doc: &Document| -> Vec<(usize, String)> {
        doc.annotations
            .lsp_diagnostics()
            .map(|a| match a.anchor {
                Anchor::Line(l) => (
                    l,
                    crate::annotations::payload::lsp::message(&a.payload)
                        .unwrap()
                        .to_string(),
                ),
                other => panic!("expected line anchor, got {:?}", other),
            })
            .collect()
    };

    doc.undo();
    assert_eq!(doc.buffer.to_string(), "aaa\nbbb\nccc");
    assert_eq!(
        diag_lines(&doc),
        vec![(1, "new".to_string())],
        "undo must not resurrect old diagnostics"
    );
    assert!(
        doc.annotations.get(user).is_some(),
        "non-LSP annotations still restore"
    );

    doc.redo();
    assert_eq!(
        diag_lines(&doc),
        vec![(1, "new".to_string())],
        "redo must keep the live diagnostics"
    );
}
