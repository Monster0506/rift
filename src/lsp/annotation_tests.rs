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
