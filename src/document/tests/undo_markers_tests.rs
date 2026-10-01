use super::common::*;
use super::*;

#[test]
fn test_undo_binary_data() {
    let mut doc = Document::new(1).unwrap();
    let binary_char = crate::character::Character::Byte(0xFF);
    doc.buffer.insert_character(binary_char).unwrap();

    assert_eq!(doc.buffer.len(), 1);
    assert_eq!(doc.buffer.char_at(0), Some(binary_char));

    doc.delete_range(0, 1).unwrap();
    assert_eq!(doc.buffer.len(), 0);

    doc.undo();

    assert_eq!(doc.buffer.len(), 1);
    assert_eq!(doc.buffer.char_at(0), Some(binary_char));
}

#[test]
#[should_panic(expected = "stale history Position")]
fn test_undo_with_stale_position_panics_in_debug_instead_of_silently_corrupting() {
    use crate::history::{EditOperation, EditTransaction, Position, Range};

    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello").unwrap();

    let mut tx = EditTransaction::new("bogus");
    tx.record(EditOperation::Delete {
        range: Range::new(Position::new(99, 0), Position::new(99, 1)),
        deleted_text: vec![crate::character::Character::from('x')],
    });
    doc.history.push(tx, None);

    doc.undo();
}

#[test]
fn test_undo_of_non_transactional_delete_restores_cursor_onto_deleted_char() {
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello world").unwrap();
    doc.buffer.set_cursor(0).unwrap();

    assert!(doc.delete_forward());
    assert_eq!(doc.buffer.to_string(), "ello world");

    doc.undo();
    assert_eq!(doc.buffer.to_string(), "hello world");
    assert_eq!(
        doc.buffer.cursor(),
        0,
        "undo must land the cursor back on the restored char, not past it"
    );
}

#[test]
fn test_redo_of_non_transactional_insert_restores_cursor_after_inserted_text() {
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello").unwrap();
    doc.buffer.set_cursor(0).unwrap();
    doc.insert_str("XY").unwrap();
    assert_eq!(doc.buffer.to_string(), "XYhello");
    assert_eq!(doc.buffer.cursor(), 2);

    doc.undo();
    assert_eq!(doc.buffer.to_string(), "hello");

    doc.redo();
    assert_eq!(doc.buffer.to_string(), "XYhello");
    assert_eq!(
        doc.buffer.cursor(),
        2,
        "redo must restore the cursor to exactly where the edit left it"
    );
}

#[test]
fn test_get_changed_line_for_seq() {
    let mut doc = Document::new(1).unwrap();

    assert_eq!(doc.get_changed_line_for_seq(0), None);

    doc.insert_str("hello\n").unwrap();
    let seq1 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq1), Some(0));

    doc.insert_str("world\n").unwrap();
    let seq2 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq2), Some(1));

    doc.insert_str("line3").unwrap();
    let seq3 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq3), Some(2));

    doc.buffer.set_cursor(0).unwrap(); // Go to start
    doc.delete_forward(); // Delete 'h'
    let seq4 = doc.history.current_seq();
    assert_eq!(doc.get_changed_line_for_seq(seq4), Some(0));

    assert_eq!(doc.get_changed_line_for_seq(9999), None);
}

#[test]
fn test_undo_redo_restores_annotation_marker_positions() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello world").unwrap();
    let id = doc.annotations.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::range(6, 11),
        AnnotationOwner::User,
    ));
    let span = |doc: &Document| match doc.annotations.get(id).unwrap().anchor {
        Anchor::Range(s, e) => (s.offset, e.offset),
        other => panic!("expected range, got {:?}", other),
    };

    doc.buffer.set_cursor(0).ok();
    doc.insert_str("XY").unwrap();
    assert_eq!(span(&doc), (8, 13));

    doc.undo();
    assert_eq!(span(&doc), (6, 11));
    doc.redo();
    assert_eq!(span(&doc), (8, 13));
}

#[test]
fn test_goto_seq_restores_correctly_via_checkpoint_snapshot() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};

    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello world").unwrap();
    doc.annotations.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::range(6, 11),
        AnnotationOwner::User,
    ));
    doc.checkpoint();
    let checkpoint_seq = doc.history.current_seq();

    for _ in 0..20 {
        doc.insert_str("!").unwrap();
    }
    assert_eq!(doc.buffer.to_string(), "hello world!!!!!!!!!!!!!!!!!!!!");

    doc.goto_seq(checkpoint_seq).unwrap();

    assert_eq!(doc.buffer.to_string(), "hello world");
    let span = match doc.annotations.iter().next().unwrap().anchor {
        Anchor::Range(s, e) => (s.offset, e.offset),
        other => panic!("expected range, got {:?}", other),
    };
    assert_eq!(span, (6, 11));
}

#[test]
fn test_goto_seq_keeps_annotation_stacks_in_sync() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello world").unwrap();
    let seq_after_first_edit = doc.history.current_seq();

    let id = doc.annotations.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::range(6, 11),
        AnnotationOwner::User,
    ));
    let span = |doc: &Document| match doc.annotations.get(id).unwrap().anchor {
        Anchor::Range(s, e) => (s.offset, e.offset),
        other => panic!("expected range, got {:?}", other),
    };

    doc.buffer.set_cursor(0).ok();
    doc.insert_str("XY").unwrap();
    assert_eq!(span(&doc), (8, 13));

    doc.goto_seq(seq_after_first_edit).unwrap();
    assert_eq!(
        span(&doc),
        (6, 11),
        "goto_seq must revert the annotation shift like undo() does"
    );

    doc.redo();
    assert_eq!(
        span(&doc),
        (8, 13),
        "redo() after goto_seq must use the entry goto_seq pushed, not desync"
    );
}

#[test]
fn test_diff_undo_matches_snapshot_for_insertions() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind, Stickiness};

    fn seed(doc: &mut Document) {
        doc.insert_str("line0\nline1\nline2\nline3").unwrap();
        doc.annotations.add(Annotation::new(
            Kind::new("ui.point"),
            Anchor::point(3),
            AnnotationOwner::User,
        ));
        doc.annotations.add(Annotation::new(
            Kind::new("ui.range"),
            Anchor::range(7, 11),
            AnnotationOwner::User,
        ));
        doc.annotations.add(
            Annotation::new(
                Kind::new("ui.persist"),
                Anchor::range(13, 17),
                AnnotationOwner::User,
            )
            .with_stickiness(Stickiness::Persist),
        );
        doc.annotations.create_diagnostic(2, 1, "err");
        doc.annotations.create_diagnostic(3, 2, "warn");
    }

    type EditFn = fn(&mut Document);
    let cases: Vec<EditFn> = vec![
        |d| {
            d.buffer.set_cursor(1).ok();
            d.insert_char('X').unwrap();
        },
        |d| {
            d.buffer.set_cursor(0).ok();
            d.insert_char('\n').unwrap();
        },
        |d| {
            d.buffer.set_cursor(9).ok();
            d.insert_str("a\nb\nc").unwrap();
        },
        |d| {
            d.buffer.set_cursor(7).ok();
            d.insert_char('Z').unwrap();
        },
    ];

    for case in cases {
        let mut doc = Document::new(1).unwrap();
        seed(&mut doc);
        let before = doc.annotations.snapshot();
        case(&mut doc);
        let after = doc.annotations.snapshot();
        assert_ne!(before, after, "edit should move annotations");

        doc.undo();
        assert_eq!(
            doc.annotations.snapshot(),
            before,
            "undo must restore the exact pre-edit annotation state"
        );

        doc.redo();
        assert_eq!(
            doc.annotations.snapshot(),
            after,
            "redo must restore the exact post-edit annotation state"
        );
    }
}

#[test]
fn test_diff_undo_interleaves_with_delete_snapshot_path() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};

    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello world").unwrap();
    let id = doc.annotations.add(Annotation::new(
        Kind::new("ui.range"),
        Anchor::range(6, 11),
        AnnotationOwner::User,
    ));
    let span = |doc: &Document| match doc.annotations.get(id).map(|a| a.anchor) {
        Some(Anchor::Range(s, e)) => Some((s.offset, e.offset)),
        _ => None,
    };
    assert_eq!(span(&doc), Some((6, 11)));

    doc.buffer.set_cursor(0).ok();
    doc.insert_str("XY").unwrap();
    assert_eq!(span(&doc), Some((8, 13)));

    doc.buffer.set_cursor(0).ok();
    doc.delete_forward();
    doc.delete_forward();
    assert_eq!(span(&doc), Some((6, 11)));

    doc.undo();
    doc.undo();
    assert_eq!(span(&doc), Some((8, 13)));
    doc.undo();
    assert_eq!(span(&doc), Some((6, 11)));

    doc.redo();
    assert_eq!(span(&doc), Some((8, 13)));
    doc.redo();
    doc.redo();
    assert_eq!(span(&doc), Some((6, 11)));
}

#[test]
fn test_line_anchor_tracks_newline_edits_in_normal_buffer() {
    use crate::annotations::Anchor;
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("line0\nline1\nline2").unwrap();
    doc.annotations.create_diagnostic(2, 1, "err");

    doc.buffer.set_cursor(0).ok();
    doc.insert_char('\n').unwrap();
    assert_eq!(
        doc.annotations.lsp_diagnostics().next().unwrap().anchor,
        Anchor::Line(3)
    );

    doc.buffer.set_cursor(1).ok();
    doc.delete_backward();
    assert_eq!(
        doc.annotations.lsp_diagnostics().next().unwrap().anchor,
        Anchor::Line(2)
    );
}

#[test]
fn test_range_anchor_markers_track_edits() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello world here").unwrap();
    let id = doc.annotations.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::range(6, 11),
        AnnotationOwner::User,
    ));

    doc.buffer.set_cursor(0).ok();
    doc.insert_str("XY").unwrap();
    match doc.annotations.get(id).unwrap().anchor {
        Anchor::Range(s, e) => {
            assert_eq!(s.offset, 8);
            assert_eq!(e.offset, 13);
        }
        other => panic!("expected range, got {:?}", other),
    }

    doc.buffer.set_cursor(10).ok();
    doc.insert_str("Z").unwrap();
    match doc.annotations.get(id).unwrap().anchor {
        Anchor::Range(s, e) => {
            assert_eq!(s.offset, 8);
            assert_eq!(e.offset, 14);
        }
        other => panic!("expected range, got {:?}", other),
    }
}

#[test]
fn test_range_anchor_delete_stickiness_removes_annotation() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("abcdefgh").unwrap();
    let id = doc.annotations.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::range(2, 5),
        AnnotationOwner::User,
    ));
    doc.delete_range(1, 6).unwrap();
    assert!(doc.annotations.get(id).is_none());
}

#[test]
fn test_document_version_increments_per_edit() {
    let mut doc = Document::new(1).unwrap();
    assert_eq!(doc.version(), 0);
    doc.insert_str("a").unwrap();
    let v1 = doc.version();
    assert!(v1 > 0);
    doc.insert_str("b").unwrap();
    assert!(doc.version() > v1);
}

#[test]
fn test_delete_range_mid_line_merges_line_anchors_into_start_line() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind, Stickiness};
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("aaa\nbbb\nccc\nddd").unwrap();
    let persist = |line| {
        Annotation::new(
            Kind::new("a.persist"),
            Anchor::Line(line),
            AnnotationOwner::User,
        )
        .with_stickiness(Stickiness::Persist)
    };
    let on_2 = doc.annotations.add(persist(2));
    let on_3 = doc.annotations.add(persist(3));

    doc.delete_range(5, 9).unwrap();
    assert_eq!(doc.buffer.to_string(), "aaa\nbcc\nddd");
    assert_eq!(line_anchor(&doc, on_2), Some(1));
    assert_eq!(line_anchor(&doc, on_3), Some(2));
}

#[test]
fn test_delete_range_at_column_zero_removes_whole_line() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind, Stickiness};
    let mut doc = Document::new(1).unwrap();
    doc.insert_str("aaa\nbbb\nccc").unwrap();
    let gone = doc.annotations.add(
        Annotation::new(Kind::new("a.del"), Anchor::Line(1), AnnotationOwner::User)
            .with_stickiness(Stickiness::Delete),
    );
    let kept = doc.annotations.add(
        Annotation::new(Kind::new("a.keep"), Anchor::Line(2), AnnotationOwner::User)
            .with_stickiness(Stickiness::Persist),
    );

    doc.delete_range(4, 8).unwrap();
    assert_eq!(doc.buffer.to_string(), "aaa\nccc");
    assert!(doc.annotations.get(gone).is_none());
    assert_eq!(line_anchor(&doc, kept), Some(1));
}

#[test]
fn test_get_edit_points_matches_individual_calls_col0() {
    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("hello\nworld\n").unwrap();

    let offset = 0;
    let (ts_pt, hist_pos) = doc.get_edit_points(offset);

    assert_eq!(ts_pt.0, 0);
    assert_eq!(ts_pt.1, 0);
    assert_eq!(hist_pos.line, 0);
    assert_eq!(hist_pos.col, 0);
}

#[test]
fn test_get_edit_points_matches_individual_calls_midline() {
    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("hello\nworld\n").unwrap();

    let offset = 3;
    let (ts_pt, hist_pos) = doc.get_edit_points(offset);

    assert_eq!(ts_pt.0, 0);
    assert_eq!(ts_pt.1, 3);
    assert_eq!(hist_pos.line, 0);
    assert_eq!(hist_pos.col, 3);
}

#[test]
fn test_get_edit_points_matches_individual_calls_second_line() {
    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("hello\nworld\n").unwrap();

    let offset = 6;
    let (ts_pt, hist_pos) = doc.get_edit_points(offset);

    assert_eq!(ts_pt.0, 1);
    assert_eq!(ts_pt.1, 0);
    assert_eq!(hist_pos.line, 1);
    assert_eq!(hist_pos.col, 0);
}

#[test]
fn test_get_edit_points_multibyte_prefix_diverges_byte_and_char_columns() {
    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("e\u{301}llo\nworld\n").unwrap();

    let world_char_start = 6;
    let byte_offset = doc.buffer.char_to_byte(world_char_start);
    let (ts_pt, hist_pos) = doc.get_edit_points(byte_offset);

    assert_eq!(ts_pt.0, 1);
    assert_eq!(ts_pt.1, 0);
    assert_eq!(hist_pos.line, 1);
    assert_eq!(hist_pos.col, 0);
}

#[test]
fn test_insert_char_after_multibyte_prefix_shifts_annotation_by_bytes() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind, Marker};

    let mut doc = Document::new(1).unwrap();
    doc.insert_str("e\u{301}llo world").unwrap();

    let id = doc.annotations.add(Annotation::new(
        Kind::new("test.marker"),
        Anchor::Point(Marker::right(6)),
        AnnotationOwner::User,
    ));

    doc.buffer.set_cursor(6).ok();
    doc.insert_char('X').unwrap();

    let shifted = doc
        .annotations
        .query_range(0, doc.buffer.to_string().len())
        .find(|a| a.id == id)
        .expect("annotation must still exist");
    match shifted.anchor {
        Anchor::Point(marker) => assert_eq!(marker.offset, 6),
        _ => panic!("expected point anchor"),
    }
}

#[test]
fn test_delete_backward_after_multibyte_prefix_shifts_annotation_by_bytes() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};

    let mut doc = Document::new(1).unwrap();
    doc.insert_str("e\u{301}llo world").unwrap();

    let world_byte_start = doc.buffer.char_to_byte(6);
    let id = doc.annotations.add(Annotation::new(
        Kind::new("test.marker"),
        Anchor::point(world_byte_start),
        AnnotationOwner::User,
    ));

    doc.buffer.set_cursor(6).ok();
    doc.delete_backward();

    let shifted = doc
        .annotations
        .query_range(0, doc.buffer.to_string().len())
        .find(|a| a.id == id)
        .expect("annotation must still exist");
    match shifted.anchor {
        Anchor::Point(marker) => assert_eq!(marker.offset, world_byte_start - 1),
        _ => panic!("expected point anchor"),
    }
}

#[test]
fn test_delete_forward_after_multibyte_prefix_shifts_annotation_by_bytes() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};

    let mut doc = Document::new(1).unwrap();
    doc.insert_str("e\u{301}llo world").unwrap();

    let world_byte_start = doc.buffer.char_to_byte(6);
    let id = doc.annotations.add(Annotation::new(
        Kind::new("test.marker"),
        Anchor::point(world_byte_start),
        AnnotationOwner::User,
    ));

    doc.buffer.set_cursor(5).ok();
    doc.delete_forward();

    let shifted = doc
        .annotations
        .query_range(0, doc.buffer.to_string().len())
        .find(|a| a.id == id)
        .expect("annotation must still exist");
    match shifted.anchor {
        Anchor::Point(marker) => assert_eq!(marker.offset, world_byte_start - 1),
        _ => panic!("expected point anchor"),
    }
}

#[test]
fn test_line_adornment_resolves_correct_line_past_multibyte_prefix() {
    use crate::annotations::presentation::{Adornment, Placement, Presentation};
    use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind};

    let mut doc = Document::new(1).unwrap();
    doc.insert_str("ab\u{2014}\nab\u{2014}\nab\u{2014}\n---\nzzzzz\n")
        .unwrap();

    let rule_line = 3;
    let rule_char_start = doc.buffer.line_index.get_start(rule_line).unwrap();
    let rule_byte_start = doc.buffer.char_to_byte(rule_char_start);
    doc.annotations.add(
        Annotation::new(
            Kind::new("test.rule"),
            Anchor::point(rule_byte_start),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("-", Placement::Trailing)),
        ),
    );

    let adornments = doc.annotations.line_adornments(
        None,
        None,
        0..doc.buffer.to_string().len(),
        0..doc.buffer.get_total_lines(),
        true,
        |b| {
            doc.buffer
                .line_index
                .get_line_at(doc.buffer.byte_to_char(b))
        },
    );

    assert_eq!(adornments.len(), 1);
    assert_eq!(adornments[0].0, rule_line);
}

#[test]
fn undo_clears_selection_set() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("hello world").unwrap();
    doc.selection_set
        .bank(Region::new(0, 4, RangeKind::Charwise));
    assert!(!doc.selection_set.is_empty());

    doc.insert_char('!').unwrap();
    doc.undo();

    assert!(
        doc.selection_set.is_empty(),
        "undo must clear a banked selection set"
    );
}

#[test]
fn redo_clears_selection_set() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(1).unwrap();
    doc.insert_str("hello world").unwrap();
    assert!(doc.undo());
    doc.selection_set
        .bank(Region::new(0, 4, RangeKind::Charwise));
    assert!(!doc.selection_set.is_empty());

    assert!(doc.redo());

    assert!(
        doc.selection_set.is_empty(),
        "redo must clear a banked selection set"
    );
}
