use crate::annotations::{payload, Anchor, AnnotationOwner, AnnotationStore};

#[test]
fn create_lsp_diagnostic_stores_annotation() {
    let mut store = AnnotationStore::new();
    let id = store.create_lsp_diagnostic(5, "[error] type mismatch".into());
    assert!(id > 0);

    let diags: Vec<_> = store.lsp_diagnostics().collect();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].anchor, Anchor::Line(5));
    assert_eq!(
        payload::tooltip(&diags[0].payload),
        Some("[error] type mismatch")
    );
    assert_eq!(diags[0].kind.as_str(), "lsp.diagnostic");
    assert_eq!(diags[0].owner, AnnotationOwner::Lsp);
}

#[test]
fn clear_lsp_diagnostics_removes_only_lsp_annotations() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(0, 1); // non-LSP annotation
    store.create_lsp_diagnostic(3, "error on line 3".into());
    store.create_lsp_diagnostic(7, "error on line 7".into());

    assert_eq!(store.lsp_diagnostics().count(), 2);

    store.clear_lsp_diagnostics();

    assert_eq!(store.lsp_diagnostics().count(), 0);
    // Directory entry should survive
    assert_eq!(store.directory_entries_by_line().len(), 1);
}

#[test]
fn multiple_diagnostics_on_different_lines() {
    let mut store = AnnotationStore::new();
    for line in 0..10 {
        store.create_lsp_diagnostic(line, format!("error at line {}", line));
    }
    assert_eq!(store.lsp_diagnostics().count(), 10);
}

#[test]
fn lsp_diagnostics_survive_line_insertion() {
    let mut store = AnnotationStore::new();
    store.create_lsp_diagnostic(4, "error".into());

    // Insert a line before line 4 → diagnostic shifts to line 5
    store.on_line_inserted(3);

    let diags: Vec<_> = store.lsp_diagnostics().collect();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].anchor, Anchor::Line(5));
}

#[test]
fn lsp_diagnostics_survive_line_deletion_outside_range() {
    let mut store = AnnotationStore::new();
    store.create_lsp_diagnostic(10, "error".into());

    // Delete lines 0–4 (5 lines) → diagnostic shifts to line 5
    store.on_lines_deleted(0, 5);

    let diags: Vec<_> = store.lsp_diagnostics().collect();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].anchor, Anchor::Line(5));
}

#[test]
fn replace_lsp_diagnostics_swaps_the_whole_set() {
    let mut store = AnnotationStore::new();
    store.create_directory_entry(0, 1); // non-LSP, must survive
    store.create_diagnostic(2, 1, "old error");
    store.create_diagnostic(5, 2, "old warning");
    assert_eq!(store.lsp_diagnostics().count(), 2);

    store.replace_lsp_diagnostics(vec![(3, 1, "new error"), (8, 4, "new hint")]);

    let diags: Vec<_> = store.lsp_diagnostics().collect();
    assert_eq!(diags.len(), 2);
    let lines: Vec<_> = diags
        .iter()
        .map(|a| match a.anchor {
            Anchor::Line(l) => l,
            _ => panic!("expected line anchor"),
        })
        .collect();
    assert!(lines.contains(&3) && lines.contains(&8));
    // Non-LSP annotation untouched.
    assert_eq!(store.directory_entries_by_line().len(), 1);
}

/// Clearing diagnostics with no replacement must not leave the `by_id`/line
/// index dangling; a query afterward must reflect the removal.
#[test]
fn replace_lsp_diagnostics_with_empty_clears_stale_index() {
    let mut store = AnnotationStore::new();
    store.create_diagnostic(2, 1, "err");
    store.create_diagnostic(4, 1, "err2");

    // Force the index (incl. by_id / line bucket) to build and go clean.
    let _ = store.next_interactive(0);

    // Empty replacement: clears all, must invalidate so the index rebuilds.
    store.replace_lsp_diagnostics(Vec::<(usize, i64, &str)>::new());

    assert_eq!(store.lsp_diagnostics().count(), 0);
    // A line-anchor edit consults the (now-rebuilt) index without stale ids.
    store.on_line_inserted(0);
    assert_eq!(store.lsp_diagnostics().count(), 0);
}

#[test]
fn lsp_diagnostics_persist_when_their_line_is_deleted() {
    // LspDiagnostics use Stickiness::Persist, so deleting their line keeps them
    let mut store = AnnotationStore::new();
    store.create_lsp_diagnostic(3, "error".into());

    store.on_lines_deleted(3, 1);

    // Should persist (moved to nearest line, which is still line 3 after deletion
    // since Persist keeps it at the same numeric position clamped to valid range)
    let diags: Vec<_> = store.lsp_diagnostics().collect();
    assert_eq!(
        diags.len(),
        1,
        "diagnostic should survive Persist stickiness"
    );
}
