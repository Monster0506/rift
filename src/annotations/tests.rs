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
    assert!(store.presentation_spans(None, None).is_empty());
    assert_eq!(
        store.presentation_spans(None, Some(&defaults)),
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
        store.presentation_spans(None, Some(&defaults)),
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
        store.presentation_spans(None, None),
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
        store.presentation_spans(None, None),
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
    assert!(store.presentation_spans(None, None).is_empty());
}
