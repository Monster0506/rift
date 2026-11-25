use super::*;
use crate::document::ops::{Edit, EditSource};

#[cfg(feature = "lsp")]
#[test]
fn test_lsp_char_offset_handles_astral_and_non_ascii() {
    use crate::lsp::protocol::PositionEncoding;
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("a🦀é-end"), EditSource::User);

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("a🦀é-end"), EditSource::User);

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("ac"), EditSource::User);
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup
    let _ = doc.buffer.set_cursor(1);
    let _ = doc.edit(Edit::type_text('b'), EditSource::User);

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("abc"), EditSource::User);
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup
    let _ = doc.buffer.set_cursor(2);
    doc.edit(Edit::DeleteBackward, EditSource::User).unwrap();

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("🦀x"), EditSource::User);
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup
    let _ = doc.buffer.set_cursor(2);
    doc.edit(Edit::DeleteBackward, EditSource::User).unwrap();

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("a"), EditSource::User);
    let _ = doc.edit(Edit::type_text("b"), EditSource::User);

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("a"), EditSource::User);
    let _ = doc.edit(Edit::type_text("b"), EditSource::User);
    let _ = doc.edit(Edit::type_text("c"), EditSource::User);

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("x\n"), EditSource::User);
    let _ = doc.edit(Edit::type_text("y"), EditSource::User);

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("aaaa"), EditSource::User);
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup

    let _ = doc.buffer.set_cursor(0);
    let _ = doc.edit(Edit::type_text('X'), EditSource::User); // now: "Xaaaa", cursor after X
    let _ = doc.buffer.set_cursor(5);
    let _ = doc.edit(Edit::type_text('Y'), EditSource::User); // unrelated position, doesn't chain from X's insert

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("ab"), EditSource::User);
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup

    let _ = doc.edit(Edit::type_text('c'), EditSource::User); // "abc"
    doc.edit(Edit::DeleteBackward, EditSource::User).unwrap(); // back to "ab" -- one insert, one delete pending

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("abc"), EditSource::User);
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup

    doc.edit(Edit::DeleteBackward, EditSource::User).unwrap();
    doc.edit(Edit::DeleteBackward, EditSource::User).unwrap();

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
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let _ = doc.edit(Edit::type_text("ab\ncd"), EditSource::User);
    let _ = doc.take_incremental_lsp_changes(PositionEncoding::Utf16); // drain setup
    let _ = doc.buffer.set_cursor(0);
    let _ = doc.edit(Edit::delete(0..3), EditSource::User); // removes "ab\n", spanning a line break

    assert!(
        doc.take_incremental_lsp_changes(PositionEncoding::Utf16)
            .is_none(),
        "a delete spanning a newline must fall back to full sync"
    );
}

#[test]
fn test_undo_redo_keep_current_lsp_diagnostics() {
    use crate::annotations::kind::Kind;
    use crate::annotations::{Anchor, Annotation, AnnotationOwner};
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.edit(Edit::type_text("aaa\nbbb\nccc"), EditSource::User)
        .unwrap();
    let user = doc.annotations.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::range(0, 3),
        AnnotationOwner::User,
    ));
    doc.annotations
        .replace_lsp_diagnostics(vec![(0, None, 1, "old")]);

    doc.edit(Edit::delete(0..4), EditSource::User).unwrap();
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
                other => panic!("expected line anchor, got {other:?}"),
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
