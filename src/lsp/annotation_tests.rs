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

    // Insert a line before line 4 -> diagnostic shifts to line 5
    store.on_line_inserted(3);

    let diags: Vec<_> = store.lsp_diagnostics().collect();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].anchor, Anchor::Line(5));
}

#[test]
fn lsp_diagnostics_survive_line_deletion_outside_range() {
    let mut store = AnnotationStore::new();
    store.create_lsp_diagnostic(10, "error".into());

    // Delete lines 0 through 4; the diagnostic shifts to line 5.
    store.on_lines_deleted(0, 5, 0);

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

    store.replace_lsp_diagnostics(vec![(3, None, 1, "new error"), (8, None, 4, "new hint")]);

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
    store.replace_lsp_diagnostics(Vec::<crate::annotations::LspDiagnosticSpec>::new());

    assert_eq!(store.lsp_diagnostics().count(), 0);
    // A line-anchor edit consults the (now-rebuilt) index without stale ids.
    store.on_line_inserted(0);
    assert_eq!(store.lsp_diagnostics().count(), 0);
}

#[test]
fn lsp_diagnostics_persist_when_their_line_is_deleted() {
    // LspDiagnostics use Stickiness::Persist: deleting their line relocates
    // them to the merge line instead of dropping them.
    let mut store = AnnotationStore::new();
    store.create_lsp_diagnostic(3, "error".into());
    store.create_lsp_diagnostic(6, "later".into());

    // Lines 3..5 deleted from mid-line 2: their content collapsed into line 2.
    store.on_lines_deleted(3, 2, 2);

    let mut lines: Vec<usize> = store
        .lsp_diagnostics()
        .map(|a| match a.anchor {
            Anchor::Line(l) => l,
            _ => panic!("expected line anchor"),
        })
        .collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        vec![2, 4],
        "in-range moves to merge line, after shifts up"
    );

    // Whole-line delete at column 0: the diagnostic lands on the line now at 2.
    store.on_lines_deleted(2, 1, 2);
    let lines: Vec<usize> = store
        .lsp_diagnostics()
        .map(|a| match a.anchor {
            Anchor::Line(l) => l,
            _ => panic!("expected line anchor"),
        })
        .collect();
    assert_eq!(lines.iter().filter(|&&l| l == 2).count(), 1);
    assert_eq!(lines.iter().filter(|&&l| l == 3).count(), 1);
}

fn adornment_texts(store: &AnnotationStore, include_lsp: bool) -> Vec<(usize, String)> {
    store
        .line_adornments(None, None, 0..usize::MAX, 0..usize::MAX, include_lsp, |_| 0)
        .into_iter()
        .map(|(line, text, _)| (line, text.into_owned()))
        .collect()
}

#[test]
fn diagnostic_adornment_is_single_line_and_printable() {
    let mut store = AnnotationStore::new();
    store.create_diagnostic(
        0,
        1,
        "\n  mismatched\ttypes:   expected\r`i32`\nfound `&str`\n",
    );

    assert_eq!(
        adornment_texts(&store, true),
        vec![(0, "mismatched types: expected `i32`".to_string())]
    );
    // The payload keeps the whole (trimmed) message for the tooltip.
    let diag = store.lsp_diagnostics().next().unwrap();
    assert_eq!(
        payload::lsp::message(&diag.payload),
        Some("mismatched\ttypes:   expected\r`i32`\nfound `&str`")
    );
    assert_eq!(
        payload::tooltip(&diag.payload),
        Some("[error] mismatched\ttypes:   expected\r`i32`\nfound `&str`")
    );
}

#[test]
fn diagnostics_on_one_line_show_most_severe_with_count() {
    let mut store = AnnotationStore::new();
    store.create_diagnostic(3, 4, "hint");
    store.create_diagnostic(3, 2, "warn");
    store.create_diagnostic(3, 1, "err");
    store.create_diagnostic(7, 3, "info");

    assert_eq!(
        adornment_texts(&store, true),
        vec![(3, "err (+2)".to_string()), (7, "info".to_string())]
    );
    assert_eq!(
        store.tooltip_at_line(3, None, true, |_| 0),
        Some("[error] err"),
        "tooltip prefers the most severe diagnostic"
    );
}

#[test]
fn lsp_adornments_and_tooltips_can_be_hidden() {
    let mut store = AnnotationStore::new();
    store.create_diagnostic(3, 1, "err");

    assert!(adornment_texts(&store, false).is_empty());
    assert_eq!(store.tooltip_at_line(3, None, false, |_| 0), None);
    assert_eq!(
        store.tooltip_at_line(3, None, true, |_| 0),
        Some("[error] err")
    );
}

#[test]
fn ranged_diagnostics_underline_their_span_and_still_resolve_by_line() {
    let mut store = AnnotationStore::new();
    store.replace_lsp_diagnostics(vec![
        (2, Some(10..14), 2, "warn"),
        (4, Some(30..30), 1, "err"),
    ]);

    let diags: Vec<_> = store.lsp_diagnostics().collect();
    assert_eq!(diags[0].anchor, Anchor::range(10, 14));
    // An empty span degrades to the line anchor (nothing to underline).
    assert_eq!(diags[1].anchor, Anchor::Line(4));

    // Underline only: syntax colors are untouched (no fg/bg in the style).
    let spans = store.presentation_spans(None, None, 0..100);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].0, 10..14);
    assert!(spans[0].1.attrs.underline);
    assert_eq!(spans[0].1.fg, None);

    // Line-based consumers map the span start through `line_of`.
    let line_of = |b: usize| if b >= 10 { 2 } else { 0 };
    let texts: Vec<(usize, String)> = store
        .line_adornments(None, None, 0..usize::MAX, 0..usize::MAX, true, line_of)
        .into_iter()
        .map(|(line, text, _)| (line, text.into_owned()))
        .collect();
    assert_eq!(texts, vec![(2, "warn".to_string()), (4, "err".to_string())]);
    assert_eq!(
        store.tooltip_at_line(2, None, true, line_of),
        Some("[warning] warn")
    );
    assert_eq!(store.tooltip_at_line(2, None, false, line_of), None);
    assert_eq!(store.tooltip_at(12, None, false), None);
    assert_eq!(store.tooltip_at(12, None, true), Some("[warning] warn"));
}
