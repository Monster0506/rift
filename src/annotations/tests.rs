use super::*;

#[test]
fn test_create_directory_entry_assigns_unique_ids() {
    let mut store = AnnotationStore::new();
    let id1 = store.create_directory_entry(1, 10);
    let id2 = store.create_directory_entry(2, 20);
    assert_ne!(id1, id2);
    assert!(id1 > 0);
    assert!(id2 > id1);
}

#[test]
fn test_add_with_id_keeps_next_id_ahead() {
    let mut store = AnnotationStore::new();
    let a = store.add(Annotation::new(
        Kind::new("a.x"),
        Anchor::point(0),
        AnnotationOwner::User,
    ));
    assert_eq!(a, 1);
    assert_eq!(store.peek_next_id(), 2);
    // Insert under a pre-claimed higher id; next_id jumps past it.
    store.add_with_id(
        7,
        Annotation::new(Kind::new("a.y"), Anchor::point(1), AnnotationOwner::User),
    );
    assert_eq!(store.peek_next_id(), 8);
    assert!(store.get(7).is_some());
    // A later auto-allocated id does not collide with the pre-claimed one.
    let c = store.add(Annotation::new(
        Kind::new("a.z"),
        Anchor::point(2),
        AnnotationOwner::User,
    ));
    assert_eq!(c, 8);
}

#[test]
fn test_directory_entry_id_at_line_hit() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(3, 42);
    assert_eq!(store.directory_entry_id_at_line(3), Some(42));
}

#[test]
fn test_directory_entry_id_at_line_miss() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(3, 42);
    assert_eq!(store.directory_entry_id_at_line(0), None);
    assert_eq!(store.directory_entry_id_at_line(2), None);
    assert_eq!(store.directory_entry_id_at_line(4), None);
}

#[test]
fn test_directory_entries_by_line_sorted() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(5, 50);
    store.create_directory_entry(1, 10);
    store.create_directory_entry(3, 30);
    let entries = store.directory_entries_by_line();
    assert_eq!(entries, vec![(1, 10), (3, 30), (5, 50)]);
}

#[test]
fn test_on_lines_deleted_removes_in_range() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.create_directory_entry(3, 3);
    store.on_lines_deleted(2, 1);
    // Entry 2 deleted; entry 3 shifts to line 2
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (2, 3)]);
}

/// Removing Delete-sticky Line annotations on line deletion is a pure
/// line-anchor edit; it must not invalidate the Point/Range interval index.
#[test]
fn test_on_lines_deleted_removal_keeps_index_valid() {
    let mut store = AnnotationStore::new();
    let point = store.add(Annotation::new(
        Kind::new("a.point"),
        Anchor::point(0),
        AnnotationOwner::User,
    ));
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.create_directory_entry(3, 3);

    // Force the interval index to build once.
    assert_eq!(store.query_at(0).count(), 1);
    assert!(!store.is_index_dirty());

    // Deletes line 2, hitting the removal branch (Delete-sticky entry 2).
    store.on_lines_deleted(2, 1);

    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (2, 3)]);
    assert!(store.get(point).is_some());
    assert!(
        !store.is_index_dirty(),
        "line-anchor removal should not invalidate the Point/Range index"
    );
}

#[test]
fn test_on_lines_deleted_multi_count() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.create_directory_entry(3, 3);
    store.create_directory_entry(4, 4);
    store.on_lines_deleted(2, 2);
    // Entries 2 and 3 deleted; entry 4 shifts to line 2
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (2, 4)]);
}

#[test]
fn test_on_lines_deleted_before_range_unchanged() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(3, 3);
    store.on_lines_deleted(2, 1);
    let entries = store.directory_entries_by_line();
    assert_eq!(entries[0], (1, 1), "line 1 should not shift");
    assert_eq!(entries[1], (2, 3), "line 3 should shift to line 2");
}

#[test]
fn test_on_line_inserted_shifts_at_and_after() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.on_line_inserted(2);
    // Line 2 shifts to 3; line 1 unchanged
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (3, 2)]);
}

#[test]
fn test_on_line_inserted_at_zero_shifts_all() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(0, 1);
    store.create_directory_entry(1, 2);
    store.on_line_inserted(0);
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (2, 2)]);
}

#[test]
fn test_on_line_inserted_before_does_not_shift_earlier() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(3, 3);
    store.on_line_inserted(2);
    // Line 1 unchanged; line 3 shifts to 4
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1), (4, 3)]);
}

#[test]
fn test_clear_removes_all() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.create_directory_entry(2, 2);
    store.clear();
    assert!(store.directory_entries_by_line().is_empty());
    assert_eq!(store.directory_entry_id_at_line(1), None);
}

#[test]
fn test_on_lines_deleted_zero_count_is_noop() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    store.on_lines_deleted(1, 0);
    assert_eq!(store.directory_entries_by_line(), vec![(1, 1)]);
}

#[test]
fn test_multiple_annotations_same_kind_different_lines() {
    let mut store = AnnotationStore::new();
    for i in 1..=5u16 {
        store.create_directory_entry(i as usize, i);
    }
    let entries = store.directory_entries_by_line();
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[0], (1, 1));
    assert_eq!(entries[4], (5, 5));
}

#[test]
fn test_leading_width_in_counts_anchor_at_cursor() {
    use crate::annotations::{Adornment, Placement, Presentation};
    let mut store = AnnotationStore::new();
    // A 3-wide leading marker anchored at offset 5.
    store.add(
        Annotation::new(Kind::new("ui.x"), Anchor::point(5), AnnotationOwner::User)
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new(">>>", Placement::Leading)),
            ),
    );
    // Cursor strictly before the anchor (exclusive end == 5): not counted.
    assert_eq!(store.leading_width_in(0, 5), 0);
    // Cursor on the anchored char (callers pass cursor+1 == 6): counted, so the
    // cursor clears the marker instead of landing inside it.
    assert_eq!(store.leading_width_in(0, 6), 3);
}

// Interval-index queries (P5)

#[test]
fn test_index_queries_and_rebuild_after_edit() {
    let mut store = AnnotationStore::new();
    let a = store.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::range(2, 6),
        AnnotationOwner::User,
    ));
    let b = store.add(Annotation::new(
        Kind::new("ui.link"),
        Anchor::point(10),
        AnnotationOwner::User,
    ));
    assert_eq!(store.query_at(3).map(|x| x.id).collect::<Vec<_>>(), vec![a]);
    assert_eq!(
        store.query_at(10).map(|x| x.id).collect::<Vec<_>>(),
        vec![b]
    );
    assert!(store.query_at(8).next().is_none());
    assert_eq!(
        store.query_range(0, 20).map(|x| x.id).collect::<Vec<_>>(),
        vec![a, b]
    );

    // Inserting 5 bytes at offset 0 shifts both anchors; the index rebuilds.
    store.on_edit(0, 0, 5);
    assert!(store.query_at(3).next().is_none());
    assert_eq!(store.query_at(8).map(|x| x.id).collect::<Vec<_>>(), vec![a]);
    assert_eq!(
        store.query_at(15).map(|x| x.id).collect::<Vec<_>>(),
        vec![b]
    );
}

/// `update()` mutating only a non-positional field (no anchor or
/// interactivity change) must not invalidate the already-built index.
#[test]
fn test_update_non_positional_change_keeps_index_valid() {
    let mut store = AnnotationStore::new();
    let id = store.add(Annotation::new(
        Kind::new("ui.checkbox"),
        Anchor::point(5),
        AnnotationOwner::User,
    ));
    // Force the index to build once.
    assert_eq!(store.query_at(5).count(), 1);
    assert!(!store.is_index_dirty());

    // Toggle `visible` only; the anchor never moves.
    store.update(id, |a| a.visible = !a.visible);

    assert!(!store.is_index_dirty());
}

// Interactive navigation (P4)

fn interactive(kind: &str, start: usize, end: usize) -> Annotation {
    Annotation::new(
        Kind::new(kind),
        Anchor::range(start, end),
        AnnotationOwner::User,
    )
    .with_actions(vec![Action::activate()])
}

#[test]
fn test_interactive_at_finds_covering_annotation() {
    let mut store = AnnotationStore::new();
    let id = store.add(interactive("ui.link", 4, 8));
    // Non-interactive range that also covers the offset.
    store.add(Annotation::new(
        Kind::new("a.x"),
        Anchor::range(0, 20),
        AnnotationOwner::User,
    ));
    assert_eq!(store.interactive_at(5).map(|a| a.id), Some(id));
    assert!(store.interactive_at(9).is_none());
}

#[test]
fn test_next_and_prev_interactive() {
    let mut store = AnnotationStore::new();
    let a = store.add(interactive("ui.link", 2, 4));
    let b = store.add(interactive("ui.link", 10, 12));
    let c = store.add(interactive("ui.link", 20, 22));
    assert_eq!(store.next_interactive(0).map(|x| x.id), Some(a));
    assert_eq!(store.next_interactive(5).map(|x| x.id), Some(b));
    assert_eq!(store.next_interactive(20).map(|x| x.id), None);
    assert_eq!(store.prev_interactive(25).map(|x| x.id), Some(c));
    assert_eq!(store.prev_interactive(11).map(|x| x.id), Some(b));
    assert_eq!(store.prev_interactive(2).map(|x| x.id), None);
}

#[test]
fn test_next_interactive_rebuilds_after_edit() {
    // The interactive-starts index must be rebuilt after an edit shifts anchors.
    let mut store = AnnotationStore::new();
    let a = store.add(interactive("ui.link", 2, 4));
    let b = store.add(interactive("ui.link", 10, 12));
    assert_eq!(store.next_interactive(0).map(|x| x.id), Some(a));
    // Insert 5 bytes at offset 0: both anchors shift right by 5.
    store.on_edit(0, 0, 5);
    assert_eq!(store.next_interactive(0).map(|x| x.id), Some(a));
    assert_eq!(store.next_interactive(7).map(|x| x.id), Some(b));
    // a now starts at 7; nothing interactive strictly after 15.
    assert_eq!(store.next_interactive(15).map(|x| x.id), None);
}

#[test]
fn test_affordance_line_formats_keys_and_verbs() {
    use crate::annotations::KeyHint;
    let ann = Annotation::new(
        Kind::new("vcs.hunk"),
        Anchor::point(0),
        AnnotationOwner::User,
    )
    .with_actions(vec![
        Action::new("stage").as_default(),
        Action::new("discard").with_key_hint(KeyHint::new("d")),
    ]);
    assert_eq!(
        ann.affordance_line("Enter").as_deref(),
        Some("Enter: stage \u{b7} d: discard")
    );
    // No actions -> no affordance.
    let inert = Annotation::new(Kind::new("a.x"), Anchor::point(0), AnnotationOwner::User);
    assert_eq!(inert.affordance_line("Enter"), None);
}

#[test]
fn test_default_action_selection() {
    let single = Annotation::new(
        Kind::new("ui.link"),
        Anchor::point(0),
        AnnotationOwner::User,
    )
    .with_actions(vec![Action::new("run")]);
    assert_eq!(
        single.default_action().map(|a| a.verb.as_str()),
        Some("run")
    );

    let multi = Annotation::new(Kind::new("ui.x"), Anchor::point(0), AnnotationOwner::User)
        .with_actions(vec![Action::new("a"), Action::new("b").as_default()]);
    assert_eq!(multi.default_action().map(|a| a.verb.as_str()), Some("b"));
}

// Presentation (P3)

fn fg_style(fg: crate::color::Color) -> crate::layer::CellStyle {
    crate::layer::CellStyle {
        fg: Some(fg),
        bg: None,
        attrs: crate::layer::CellAttrs::default(),
    }
}

#[test]
fn test_presentation_spans_uses_kind_default_when_unset() {
    use crate::annotations::registry::KindRegistry;
    use crate::color::Color;
    let mut store = AnnotationStore::new();
    // No per-annotation presentation; the kind default should style it.
    store.add(Annotation::new(
        Kind::new("lsp.diagnostic"),
        Anchor::range(0, 3),
        AnnotationOwner::Lsp,
    ));
    let defaults = KindRegistry::with_core();
    // Without defaults: nothing. With defaults: the diag.error face (Red).
    assert!(store
        .presentation_spans(None, None, 0..usize::MAX)
        .is_empty());
    assert_eq!(
        store.presentation_spans(None, Some(&defaults), 0..usize::MAX),
        vec![(0..3, fg_style(Color::Red))]
    );
}

#[test]
fn test_presentation_spans_annotation_overrides_kind_default() {
    use crate::annotations::registry::KindRegistry;
    use crate::color::Color;
    let mut store = AnnotationStore::new();
    // An explicit presentation wins over the kind default.
    store.add(
        Annotation::new(
            Kind::new("lsp.diagnostic"),
            Anchor::range(0, 3),
            AnnotationOwner::Lsp,
        )
        .with_presentation(Presentation::with_style(StyleOverride {
            fg: Some(Color::Green),
            ..Default::default()
        })),
    );
    let defaults = KindRegistry::with_core();
    assert_eq!(
        store.presentation_spans(None, Some(&defaults), 0..usize::MAX),
        vec![(0..3, fg_style(Color::Green))]
    );
}

#[test]
fn test_presentation_spans_resolves_named_face() {
    use crate::color::Color;
    let mut store = AnnotationStore::new();
    store.add(
        Annotation::new(
            Kind::new("ui.link"),
            Anchor::range(2, 5),
            AnnotationOwner::User,
        )
        .with_presentation(Presentation::with_face(FaceRef::new("link"))),
    );
    // "link" resolves to Blue via the built-in fallback (no syntax colors).
    assert_eq!(
        store.presentation_spans(None, None, 0..usize::MAX),
        vec![(2..5, fg_style(Color::Blue))]
    );
}

#[test]
fn test_presentation_spans_higher_priority_wins_overlap() {
    use crate::color::Color;
    let mut store = AnnotationStore::new();
    let low = StyleOverride {
        fg: Some(Color::Red),
        ..Default::default()
    };
    let high = StyleOverride {
        fg: Some(Color::Green),
        ..Default::default()
    };
    store.add(
        Annotation::new(
            Kind::new("a.x"),
            Anchor::range(0, 10),
            AnnotationOwner::User,
        )
        .with_presentation(Presentation::with_style(low).with_priority(1)),
    );
    store.add(
        Annotation::new(Kind::new("b.y"), Anchor::range(3, 6), AnnotationOwner::User)
            .with_presentation(Presentation::with_style(high).with_priority(5)),
    );
    assert_eq!(
        store.presentation_spans(None, None, 0..usize::MAX),
        vec![
            (0..3, fg_style(Color::Red)),
            (3..6, fg_style(Color::Green)),
            (6..10, fg_style(Color::Red))
        ]
    );
}

#[test]
fn test_presentation_spans_skips_invisible_and_unstyled() {
    let mut store = AnnotationStore::new();
    // Invisible, even though styled.
    store.add(
        Annotation::new(Kind::new("a.x"), Anchor::range(0, 3), AnnotationOwner::User)
            .with_presentation(Presentation::with_face(FaceRef::new("link")))
            .with_visible(false),
    );
    // Visible but no presentation.
    store.add(Annotation::new(
        Kind::new("a.y"),
        Anchor::range(4, 6),
        AnnotationOwner::User,
    ));
    assert!(store
        .presentation_spans(None, None, 0..usize::MAX)
        .is_empty());
}

fn trailing_adornment(store: &mut AnnotationStore, kind: &str, text: &str, pres: Presentation) {
    store.add(
        Annotation::new(Kind::new(kind), Anchor::point(0), AnnotationOwner::User)
            .with_presentation(pres.with_adornment(Adornment::new(text, Placement::Trailing))),
    );
}

#[test]
fn test_adornment_uses_kind_default_style() {
    use crate::annotations::registry::KindRegistry;
    use crate::color::Color;
    let mut store = AnnotationStore::new();
    trailing_adornment(&mut store, "md.rule", "---", Presentation::default());
    let mut defaults = KindRegistry::new();
    defaults.set_presentation(
        "md.rule",
        Presentation::with_style(StyleOverride {
            fg: Some(Color::Grey),
            ..Default::default()
        }),
    );
    // Without defaults: DarkGrey fallback. With defaults: the kind style fg.
    assert_eq!(
        store.line_adornments(None, None, 0..usize::MAX, 0..usize::MAX, |_| 0),
        vec![(0, "---".to_string(), Color::DarkGrey)]
    );
    assert_eq!(
        store.line_adornments(None, Some(&defaults), 0..usize::MAX, 0..usize::MAX, |_| 0),
        vec![(0, "---".to_string(), Color::Grey)]
    );
}

#[test]
fn test_adornment_inline_style_wins_over_kind_default() {
    use crate::annotations::registry::KindRegistry;
    use crate::color::Color;
    let mut store = AnnotationStore::new();
    store.add(
        Annotation::new(
            Kind::new("md.rule"),
            Anchor::point(0),
            AnnotationOwner::User,
        )
        .with_presentation(Presentation::default().with_adornment(
            Adornment::new("---", Placement::Trailing).with_style(StyleOverride {
                fg: Some(Color::Cyan),
                ..Default::default()
            }),
        )),
    );
    let mut defaults = KindRegistry::new();
    defaults.set_presentation(
        "md.rule",
        Presentation::with_style(StyleOverride {
            fg: Some(Color::Grey),
            ..Default::default()
        }),
    );
    assert_eq!(
        store.line_adornments(None, Some(&defaults), 0..usize::MAX, 0..usize::MAX, |_| 0),
        vec![(0, "---".to_string(), Color::Cyan)]
    );
}

#[test]
fn test_adornment_resolves_named_face() {
    use crate::color::Color;
    let mut store = AnnotationStore::new();
    store.add(
        Annotation::new(
            Kind::new("ui.link"),
            Anchor::point(0),
            AnnotationOwner::User,
        )
        .with_presentation(Presentation::default().with_adornment(
            Adornment::new("->", Placement::Trailing).with_face(FaceRef::new("link")),
        )),
    );
    // "link" resolves to Blue via the built-in fallback (no syntax colors).
    assert_eq!(
        store.line_adornments(None, None, 0..usize::MAX, 0..usize::MAX, |_| 0),
        vec![(0, "->".to_string(), Color::Blue)]
    );
}

#[test]
fn test_adornment_falls_back_to_dark_grey() {
    use crate::color::Color;
    let mut store = AnnotationStore::new();
    trailing_adornment(&mut store, "md.rule", "---", Presentation::default());
    // No style, no face, no kind default: the unchanged DarkGrey fallback.
    assert_eq!(
        store.line_adornments(None, None, 0..usize::MAX, 0..usize::MAX, |_| 0),
        vec![(0, "---".to_string(), Color::DarkGrey)]
    );
}

/// Resyncing the cached index in place after a non-removing edit must match
/// a from-scratch rebuild, checked against a hand-computed expectation.
#[test]
fn test_resync_index_matches_full_rebuild_after_shift_edits() {
    let mut store = AnnotationStore::new();
    let mut ids = Vec::new();
    // Start well past the edit point so every marker offset strictly exceeds
    // it, giving a uniform shift with no left-gravity boundary case.
    const BASE: usize = 100;
    for i in 0..20 {
        let start = BASE + i * 10;
        let mut a = Annotation::new(
            Kind::new("bench.span"),
            Anchor::range(start, start + 5),
            AnnotationOwner::User,
        );
        if i % 3 == 0 {
            a = a.with_actions(vec![Action::activate()]);
        }
        ids.push(store.add(a));
    }
    // Force the index to build once.
    assert_eq!(store.query_at(BASE).count(), 1);

    // A run of pure shift edits (inserts before every annotation) exercises
    // the resync fast path instead of a full rebuild.
    for _ in 0..5 {
        store.on_edit(1, 1, 2);
    }

    let shift = 5;
    for (i, id) in ids.iter().enumerate() {
        let a = store.get(*id).unwrap();
        let Anchor::Range(s, e) = a.anchor else {
            panic!("expected range anchor");
        };
        let start = BASE + i * 10;
        assert_eq!(s.offset, start + shift);
        assert_eq!(e.offset, start + 5 + shift);
    }

    // query_range over the whole shifted span finds every annotation.
    let found = store.query_range(BASE, BASE + 20 * 10 + shift).count();
    assert_eq!(found, 20);

    // next_interactive still walks every third annotation correctly in order.
    let mut off = 0usize;
    let mut walked = Vec::new();
    while let Some(a) = store.next_interactive(off) {
        let Anchor::Range(s, _) = a.anchor else {
            panic!("expected range anchor");
        };
        walked.push(a.id);
        off = s.offset + 1;
    }
    let expected: Vec<AnnotationId> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 3 == 0)
        .map(|(_, id)| *id)
        .collect();
    assert_eq!(walked, expected);
}

/// A fully-deleted annotation forces the slow rebuild path rather than
/// resync; confirms removal still happens and queries stay correct.
#[test]
fn test_on_edit_removal_falls_back_to_full_rebuild() {
    let mut store = AnnotationStore::new();
    let a1 = store.add(Annotation::new(
        Kind::new("bench.span"),
        Anchor::range(0, 5),
        AnnotationOwner::User,
    ));
    let a2 = store.add(Annotation::new(
        Kind::new("bench.span"),
        Anchor::range(10, 15),
        AnnotationOwner::User,
    ));
    assert_eq!(store.query_at(0).count(), 1);

    // Deletes [0,5) entirely, removing a1 (default Delete stickiness).
    store.on_edit(0, 5, 0);

    assert!(store.get(a1).is_none());
    assert!(store.get(a2).is_some());
    assert_eq!(store.query_at(0).count(), 0);
    // a2 shifted left by 5.
    assert_eq!(store.query_at(5).count(), 1);
}

/// `on_lines_deleted`/`on_line_inserted` use the line bucket to touch only
/// Line annotations; Point/Range ones mixed in must never be touched.
#[test]
fn test_line_bucket_shift_ignores_point_range_annotations() {
    let mut store = AnnotationStore::new();

    // Many unrelated Point/Range annotations spread across the byte space;
    // these must be completely unaffected by line-anchor edit tracking.
    let mut point_range_ids = Vec::new();
    for i in 0..50 {
        let start = i * 20;
        point_range_ids.push(store.add(Annotation::new(
            Kind::new("bench.span"),
            Anchor::range(start, start + 5),
            AnnotationOwner::User,
        )));
    }

    let delete_sticky = store.create_directory_entry(2, 100); // default Delete stickiness
    let persist_in_range = store.add(
        Annotation::new(
            Kind::new("a.persist"),
            Anchor::Line(3),
            AnnotationOwner::User,
        )
        .with_stickiness(Stickiness::Persist),
    );
    let before_range = store.create_directory_entry(0, 200);
    let after_range = store.create_directory_entry(5, 300);

    // Force the index (and line bucket) to build once.
    assert_eq!(store.query_at(0).count(), 1);

    // Delete lines [2, 4): removes delete_sticky, keeps persist_in_range at
    // its old line number (3), shifts after_range down by 2 (5 -> 3).
    store.on_lines_deleted(2, 2);

    assert!(store.get(delete_sticky).is_none());
    let Anchor::Line(persist_line) = store.get(persist_in_range).unwrap().anchor else {
        panic!("expected line anchor");
    };
    assert_eq!(persist_line, 3, "Persist-in-range keeps its old line");
    let Anchor::Line(before_line) = store.get(before_range).unwrap().anchor else {
        panic!("expected line anchor");
    };
    assert_eq!(
        before_line, 0,
        "lines before the deleted range are untouched"
    );
    let Anchor::Line(after_line) = store.get(after_range).unwrap().anchor else {
        panic!("expected line anchor");
    };
    assert_eq!(
        after_line, 3,
        "lines after the deleted range shift down by count"
    );

    // None of the Point/Range annotations moved or were removed.
    for (i, id) in point_range_ids.iter().enumerate() {
        let a = store.get(*id).unwrap();
        let Anchor::Range(s, e) = a.anchor else {
            panic!("expected range anchor");
        };
        let start = i * 20;
        assert_eq!(s.offset, start);
        assert_eq!(e.offset, start + 5);
    }

    // Inserting a line at 1 leaves before_range (line 0, before the insertion
    // point) alone, and shifts after_range (now at line 3) up by 1.
    store.on_line_inserted(1);
    let Anchor::Line(before_line) = store.get(before_range).unwrap().anchor else {
        panic!("expected line anchor");
    };
    assert_eq!(before_line, 0);
    let Anchor::Line(after_line) = store.get(after_range).unwrap().anchor else {
        panic!("expected line anchor");
    };
    assert_eq!(after_line, 4);

    // Point/Range annotations still untouched after the insert too.
    for (i, id) in point_range_ids.iter().enumerate() {
        let a = store.get(*id).unwrap();
        let Anchor::Range(s, _) = a.anchor else {
            panic!("expected range anchor");
        };
        assert_eq!(s.offset, i * 20);
    }
}

/// Viewport-restricted queries must exclude annotations entirely outside the
/// queried range and include ones overlapping it, for both anchor kinds.
#[test]
fn test_viewport_restricted_queries_exclude_offscreen_annotations() {
    use crate::color::Color;

    let mut store = AnnotationStore::new();

    // Trailing adornment far off-screen (line 1000) vs. one in the viewport
    // (line 5).
    store.add(
        Annotation::new(
            Kind::new("a.offscreen"),
            Anchor::Line(1000),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("off", Placement::Trailing)),
        ),
    );
    store.add(
        Annotation::new(
            Kind::new("a.onscreen"),
            Anchor::Line(5),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("on", Placement::Trailing)),
        ),
    );
    let on_screen = store.line_adornments(None, None, 0..1_000_000, 0..10, |_| 0);
    assert_eq!(on_screen, vec![(5, "on".to_string(), Color::DarkGrey)]);

    // Inline (overlay) adornment far off-screen (byte 100000) vs. on-screen
    // (byte 10).
    let mut store = AnnotationStore::new();
    store.add(
        Annotation::new(
            Kind::new("a.x"),
            Anchor::point(100_000),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("far", Placement::Overlay)),
        ),
    );
    store.add(
        Annotation::new(Kind::new("a.x"), Anchor::point(10), AnnotationOwner::User)
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new("near", Placement::Overlay)),
            ),
    );
    let inline = store.inline_adornments(None, None, 0..100);
    assert_eq!(inline.len(), 1);
    assert_eq!(inline[0].2, "near");

    // Styled presentation span off-screen vs. on-screen.
    let mut store = AnnotationStore::new();
    store.add(
        Annotation::new(
            Kind::new("a.x"),
            Anchor::range(100_000, 100_005),
            AnnotationOwner::User,
        )
        .with_presentation(Presentation::with_style(StyleOverride {
            underline: true,
            ..Default::default()
        })),
    );
    store.add(
        Annotation::new(
            Kind::new("a.x"),
            Anchor::range(10, 15),
            AnnotationOwner::User,
        )
        .with_presentation(Presentation::with_style(StyleOverride {
            bold: true,
            ..Default::default()
        })),
    );
    let spans = store.presentation_spans(None, None, 0..100);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].0, 10..15);

    // Concealed range off-screen vs. on-screen.
    let mut store = AnnotationStore::new();
    store.add(
        Annotation::new(
            Kind::new("a.x"),
            Anchor::range(100_000, 100_005),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("", Placement::Conceal)),
        ),
    );
    store.add(
        Annotation::new(
            Kind::new("a.x"),
            Anchor::range(10, 15),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("", Placement::Conceal)),
        ),
    );
    let concealed = store.concealed_ranges(0..100);
    assert_eq!(concealed, vec![(10, 15)]);
}

// Revision counter (external staleness gate for the Lua snapshot sync)

#[test]
fn test_revision_unchanged_on_true_noop_calls() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    let rev = store.revision();

    // Zero-count line deletion and a zero-width edit are documented no-ops.
    store.on_lines_deleted(5, 0);
    store.on_line_inserted(100);
    store.undo_line_inserted(100);
    store.on_edit(3, 3, 3);
    store.update(999, |a| a.visible = false); // no such id

    assert_eq!(
        store.revision(),
        rev,
        "calls that touch nothing must not bump revision"
    );
}

#[test]
fn test_revision_bumps_on_add_remove_update() {
    let mut store = AnnotationStore::new();
    let rev0 = store.revision();

    let id = store.add(Annotation::new(
        Kind::new("a.x"),
        Anchor::point(0),
        AnnotationOwner::User,
    ));
    let rev1 = store.revision();
    assert_ne!(rev1, rev0, "add must bump revision");

    store.update(id, |a| a.visible = false);
    let rev2 = store.revision();
    assert_ne!(
        rev2, rev1,
        "update must bump revision even for non-index fields"
    );

    store.remove(id);
    let rev3 = store.revision();
    assert_ne!(rev3, rev2, "remove must bump revision");
}

#[test]
fn test_revision_bumps_on_line_shift_methods() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(2, 1);
    let rev0 = store.revision();

    store.on_lines_deleted(0, 1);
    let rev1 = store.revision();
    assert_ne!(rev1, rev0, "on_lines_deleted must bump revision");

    store.on_line_inserted(0);
    let rev2 = store.revision();
    assert_ne!(rev2, rev1, "on_line_inserted must bump revision");

    store.undo_line_inserted(0);
    let rev3 = store.revision();
    assert_ne!(rev3, rev2, "undo_line_inserted must bump revision");
}

#[test]
fn test_revision_bumps_on_edit_with_positional_annotations() {
    let mut store = AnnotationStore::new();
    store.add(Annotation::new(
        Kind::new("a.x"),
        Anchor::point(10),
        AnnotationOwner::User,
    ));
    let rev0 = store.revision();
    store.on_edit(0, 0, 5);
    assert_ne!(rev0, store.revision(), "on_edit must bump revision");
}

#[test]
fn test_revision_bumps_on_clear_and_restore() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(1, 1);
    let snapshot = store.snapshot();
    let rev0 = store.revision();

    store.clear();
    let rev1 = store.revision();
    assert_ne!(rev1, rev0, "clear must bump revision");

    store.restore(snapshot);
    let rev2 = store.revision();
    assert_ne!(rev2, rev1, "restore must bump revision");
}
