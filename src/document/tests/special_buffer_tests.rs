use super::common::*;
use super::*;
use crate::color::Color;

#[test]
fn test_populate_undotree_stores_text() {
    let mut doc = Document::new_undotree(
        crate::document::DocumentId::new(1),
        crate::document::DocumentId::new(42),
    )
    .unwrap();
    let text = "* [1] edit\n* [0] root".to_string();
    let seqs = vec![1u64, 0u64];
    let highlights = vec![];
    doc.populate_undotree_buffer(&text, seqs, highlights);
    assert_eq!(doc.buffer.to_string(), text);
}

#[test]
fn test_populate_undotree_stores_sequences() {
    let mut doc = Document::new_undotree(
        crate::document::DocumentId::new(1),
        crate::document::DocumentId::new(42),
    )
    .unwrap();
    let seqs = vec![5u64, 3u64, 1u64];
    doc.populate_undotree_buffer("text", seqs.clone(), vec![]);
    assert_eq!(doc.undotree_sequences(), Some(seqs.as_slice()));
}

#[test]
fn test_populate_undotree_stores_highlights() {
    let mut doc = Document::new_undotree(
        crate::document::DocumentId::new(1),
        crate::document::DocumentId::new(42),
    )
    .unwrap();
    let highlights = vec![(0..2, Color::Magenta), (3..5, Color::Cyan)];
    doc.populate_undotree_buffer("ab cd", vec![], highlights);
    assert_eq!(doc.custom_highlights.len(), 2);
    assert_eq!(doc.custom_highlights[0].1, Color::Magenta);
    assert_eq!(doc.custom_highlights[1].1, Color::Cyan);
}

#[test]
fn test_populate_undotree_preserves_linked_doc_id() {
    let mut doc = Document::new_undotree(
        crate::document::DocumentId::new(1),
        crate::document::DocumentId::new(99),
    )
    .unwrap();
    doc.populate_undotree_buffer("x", vec![], vec![]);
    assert_eq!(
        doc.undotree_linked_doc_id(),
        Some(crate::document::DocumentId::new(99))
    );
}

#[test]
fn test_populate_undotree_noop_on_wrong_kind() {
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.populate_undotree_buffer("text", vec![1], vec![]);
    assert_eq!(doc.buffer.to_string(), "");
}

#[test]
fn test_populate_undotree_marks_saved() {
    let mut doc = Document::new_undotree(
        crate::document::DocumentId::new(1),
        crate::document::DocumentId::new(42),
    )
    .unwrap();
    doc.populate_undotree_buffer("text", vec![], vec![]);
    assert!(!doc.is_dirty());
}

#[test]
fn test_populate_undotree_replaces_old_highlights() {
    let mut doc = Document::new_undotree(
        crate::document::DocumentId::new(1),
        crate::document::DocumentId::new(42),
    )
    .unwrap();
    doc.populate_undotree_buffer("ab", vec![], vec![(0..2, Color::Red)]);
    assert_eq!(doc.custom_highlights.len(), 1);

    doc.populate_undotree_buffer(
        "xyz",
        vec![],
        vec![
            (0..1, Color::Blue),
            (1..2, Color::Green),
            (2..3, Color::Yellow),
        ],
    );
    assert_eq!(doc.custom_highlights.len(), 3);
    assert_eq!(doc.custom_highlights[0].1, Color::Blue);
}

#[test]
fn test_remove_directory_buffer_without_dirty_check() {
    let mut manager = create_manager();
    let mut doc =
        Document::new_directory(crate::document::DocumentId::new(1), PathBuf::from("/tmp"))
            .unwrap();
    let _ = doc.buffer.insert_str("some edit");
    manager.add_document(doc);

    assert!(manager
        .remove_document(crate::document::DocumentId::new(1))
        .is_ok());
}

#[test]
fn test_remove_undotree_buffer_without_dirty_check() {
    let mut manager = create_manager();
    let doc = Document::new_undotree(
        crate::document::DocumentId::new(2),
        crate::document::DocumentId::new(1),
    )
    .unwrap();
    manager.add_document(doc);
    assert!(manager
        .remove_document(crate::document::DocumentId::new(2))
        .is_ok());
}

#[test]
fn test_populate_undotree_increments_revision() {
    let mut doc = Document::new_undotree(
        crate::document::DocumentId::new(1),
        crate::document::DocumentId::new(2),
    )
    .unwrap();
    let rev0 = doc.buffer.revision;
    doc.populate_undotree_buffer("text", vec![], vec![]);
    assert!(
        doc.buffer.revision > rev0,
        "revision should increment after populate"
    );
}

#[test]
fn sync_selection_annotations_creates_one_entry_per_banked_region_plus_active() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.buffer.insert_str("hello world").unwrap();

    let banked = vec![Region::new(0, 2, RangeKind::Charwise)];
    let active = Some(Region::new(6, 8, RangeKind::Charwise));
    doc.sync_selection_annotations(active, &banked);

    let count = doc.annotations.query_kind("ui.selection").count();
    assert_eq!(count, 2, "one banked + one active annotation");
}

#[test]
fn sync_selection_annotations_active_region_draws_through_its_face_without_own_style() {
    use crate::annotations::presentation::FaceRef;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.buffer.insert_str("hello world").unwrap();

    doc.sync_selection_annotations(Some(Region::new(0, 2, RangeKind::Charwise)), &[]);

    let active = doc
        .annotations
        .query_kind("ui.selection.active")
        .next()
        .unwrap();
    let presentation = active.presentation.as_ref().unwrap();
    assert_eq!(presentation.face, Some(FaceRef::new("ui.selection.active")));
    assert_eq!(
        presentation.style, None,
        "an own style would force an fg and hide syntax colors"
    );
}

#[test]
fn sync_selection_annotations_banked_and_active_resolve_to_the_same_style() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.buffer.insert_str("0123456789").unwrap();

    let banked = vec![
        Region::new(0, 1, RangeKind::Charwise),
        Region::new(3, 4, RangeKind::Charwise),
        Region::new(6, 7, RangeKind::Charwise),
    ];
    doc.sync_selection_annotations(Some(Region::new(9, 9, RangeKind::Charwise)), &banked);

    let colors = crate::state::UserSettings::new().syntax_colors;
    let styles: Vec<_> = doc
        .annotations
        .presentation_spans(colors.as_ref(), None, 0.."0123456789".len())
        .into_iter()
        .map(|(_, style)| style)
        .collect();
    assert_eq!(styles.len(), 4);
    assert!(styles[0].bg.is_some(), "selection must draw a background");
    assert!(
        styles.iter().all(|s| *s == styles[0]),
        "banked and active regions must share one color, not a rainbow: {styles:?}"
    );
}

#[test]
fn sync_selection_annotations_replaces_previous_call() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.buffer.insert_str("hello world").unwrap();

    doc.sync_selection_annotations(None, &[Region::new(0, 2, RangeKind::Charwise)]);
    doc.sync_selection_annotations(None, &[]);

    let count = doc.annotations.query_kind("ui.selection").count();
    assert_eq!(count, 0);
}

#[test]
fn populate_regions_buffer_writes_one_line_per_region() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut source = TextBuffer::new(20).unwrap();
    source.insert_str("hello\nworld").unwrap();
    let regions = vec![
        Region::new(0, 4, RangeKind::Charwise),  // "hello"
        Region::new(6, 10, RangeKind::Charwise), // "world"
    ];

    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.populate_regions_buffer(&source, &regions);

    let content = doc.buffer.to_string();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines[0].contains("0:0"),
        "row:col for the first region; got {:?}",
        lines[0]
    );
    assert!(lines[0].contains("hello"));
    assert!(lines[1].contains("1:0"));
    assert!(lines[1].contains("world"));
}

#[test]
fn populate_regions_buffer_with_no_regions_shows_empty_placeholder() {
    let source = TextBuffer::new(10).unwrap();
    let mut doc = Document::new(crate::document::DocumentId::new(1)).unwrap();
    doc.populate_regions_buffer(&source, &[]);

    assert_eq!(doc.buffer.to_string(), "(empty)");
}

#[test]
fn populate_git_status_buffer_renders_sections_and_codes() {
    let doc = make_git_status_doc(vec![
        staged_entry("a.rs"),
        unstaged_entry("b.rs"),
        untracked_entry("c.rs"),
        unmerged_entry("d.rs"),
    ]);
    let text = doc.buffer.to_string();
    assert!(text.contains("Unmerged paths (1)"));
    assert!(text.contains("UU d.rs"));
    assert!(text.contains("Staged changes (1)"));
    assert!(text.contains("M a.rs"));
    assert!(text.contains("Unstaged changes (1)"));
    assert!(text.contains("M b.rs"));
    assert!(text.contains("Untracked files (1)"));
    assert!(text.contains("c.rs"));
    assert!(
        !text.contains("  M c.rs"),
        "untracked entries show no status code"
    );
}

#[test]
fn populate_git_status_buffer_skips_empty_sections() {
    let doc = make_git_status_doc(vec![staged_entry("a.rs")]);
    let text = doc.buffer.to_string();
    assert!(text.contains("Staged changes (1)"));
    assert!(!text.contains("Unstaged changes"));
    assert!(!text.contains("Untracked files"));
    assert!(!text.contains("Unmerged paths"));
}
