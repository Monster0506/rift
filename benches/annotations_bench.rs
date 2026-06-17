//! Marker / annotation-store cost under heavy editing (design.md sec 16).
//!
//! The interactive-annotations redesign replaced line-only edit tracking with
//! gravity-aware markers and a lazily-rebuilt interval index. These benchmarks
//! guard the two costs that matter at interactive scale: shifting many markers
//! per edit, and re-querying the index (point/range/next-interactive) after the
//! edits invalidate it.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

use monster_rift::annotations::{
    Action, Anchor, Annotation, AnnotationOwner, AnnotationStore, Kind,
};

/// A store with `n` range annotations spread evenly across a `10*n`-byte buffer,
/// every fourth one interactive (so next/prev-interactive has work to do).
fn populated_store(n: usize) -> AnnotationStore {
    let mut store = AnnotationStore::new();
    for i in 0..n {
        let start = i * 10;
        let mut a = Annotation::new(
            Kind::new("bench.span"),
            Anchor::range(start, start + 5),
            AnnotationOwner::User,
        );
        if i % 4 == 0 {
            a = a.with_actions(vec![Action::activate()]);
        }
        store.add(a);
    }
    store
}

/// A store with `n` Point/Range annotations (untouched by line-anchor edit
/// tracking) plus a few Line ones, mirroring a diagnostics-heavy buffer.
fn populated_store_with_line_anchors(n: usize) -> AnnotationStore {
    let mut store = populated_store(n);
    for line in (0..200).step_by(20) {
        store.add(Annotation::new(
            Kind::new("lsp.diagnostic"),
            Anchor::Line(line),
            AnnotationOwner::Lsp,
        ));
    }
    store
}

fn line_anchor_edit_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotations_line_anchor_tracking");
    for &n in &[100usize, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::new("on_lines_deleted", n), &n, |b, &n| {
            b.iter_batched(
                || populated_store_with_line_anchors(n),
                |mut store| {
                    for _ in 0..50 {
                        store.on_lines_deleted(black_box(1), black_box(1));
                        store.on_line_inserted(black_box(1));
                    }
                    store
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn marker_shift_under_edits(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotations_marker_shift");
    for &n in &[100usize, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || populated_store(n),
                |mut store| {
                    for k in 0..50 {
                        let at = 5 + k;
                        store.on_edit(black_box(at), black_box(at), black_box(at + 1));
                    }
                    store
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn query_after_invalidation(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotations_query");
    for &n in &[100usize, 1_000, 10_000] {
        let span = 10 * n;
        group.bench_with_input(BenchmarkId::new("point_after_edit", n), &n, |b, &n| {
            b.iter_batched(
                || populated_store(n),
                |mut store| {
                    for k in 0..50 {
                        let at = (k * 97) % span.max(1);
                        store.on_edit(at, at, at + 1);
                        let hit = store.query_at(black_box(at)).count();
                        black_box(hit);
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("range_after_edit", n), &n, |b, &n| {
            b.iter_batched(
                || populated_store(n),
                |mut store| {
                    for k in 0..50 {
                        let at = (k * 97) % span.max(1);
                        store.on_edit(at, at, at + 1);
                        let hit = store
                            .query_range(black_box(at), black_box(at + 200))
                            .count();
                        black_box(hit);
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("next_interactive", n), &n, |b, &n| {
            b.iter_batched(
                || populated_store(n),
                |store| {
                    let mut off = 0usize;
                    let mut walked = 0usize;
                    while let Some(a) = store.next_interactive(off) {
                        off = match a.anchor {
                            Anchor::Range(s, _) => s.offset + 1,
                            Anchor::Point(p) => p.offset + 1,
                            Anchor::Line(_) => break,
                        };
                        walked += 1;
                    }
                    black_box(walked)
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn presentation_flatten(c: &mut Criterion) {
    use monster_rift::annotations::{Presentation, StyleOverride};
    let mut group = c.benchmark_group("annotations_presentation_spans");
    for &n in &[100usize, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let mut store = AnnotationStore::new();
            for i in 0..n {
                let start = i * 10;
                store.add(
                    Annotation::new(
                        Kind::new("bench.style"),
                        Anchor::range(start, start + 8),
                        AnnotationOwner::User,
                    )
                    .with_presentation(Presentation::with_style(
                        StyleOverride {
                            underline: true,
                            ..Default::default()
                        },
                    )),
                );
            }
            b.iter(|| {
                let spans =
                    store.presentation_spans(black_box(None), black_box(None), 0..usize::MAX);
                black_box(spans.len())
            })
        });
    }
    group.finish();
}

fn populated_store_for_render(n: usize) -> AnnotationStore {
    use monster_rift::annotations::{Presentation, StyleOverride};
    let mut store = AnnotationStore::new();
    for i in 0..n {
        let start = i * 10;
        store.add(
            Annotation::new(
                Kind::new("bench.style"),
                Anchor::range(start, start + 8),
                AnnotationOwner::User,
            )
            .with_presentation(Presentation::with_style(StyleOverride {
                underline: true,
                ..Default::default()
            })),
        );
    }
    for line in (0..n / 5).step_by(5) {
        store.add(Annotation::new(
            Kind::new("lsp.diagnostic"),
            Anchor::Line(line),
            AnnotationOwner::Lsp,
        ));
    }
    store
}

fn render_viewport_scroll(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotations_render_viewport");
    for &n in &[1_000usize, 10_000, 100_000] {
        let span = 10 * n;
        let viewport_bytes = 2_000; // ~40 lines at ~50 bytes/line
        let viewport_lines = 40;
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || populated_store_for_render(n),
                |store| {
                    for k in 0..50 {
                        let top_byte = (k * 977) % span.max(1);
                        let top_line = (k * 41) % (n / 5).max(1);
                        let byte_range = top_byte..(top_byte + viewport_bytes).min(span);
                        let line_range = top_line..(top_line + viewport_lines);
                        let spans = store.presentation_spans(None, None, byte_range.clone());
                        let adornments = store.line_adornments(
                            None,
                            None,
                            byte_range.clone(),
                            line_range,
                            |_| 0,
                        );
                        let inline = store.inline_adornments(None, None, byte_range.clone());
                        let concealed = store.concealed_ranges(byte_range);
                        black_box((spans.len(), adornments.len(), inline.len(), concealed.len()));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn render_full_document_scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotations_render_full_scan");
    {
        let &n = &1_000usize;
        let span = 10 * n;
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || populated_store_for_render(n),
                |store| {
                    for _ in 0..50 {
                        let full = 0..span.max(1);
                        let full_lines = 0..usize::MAX;
                        let spans = store.presentation_spans(None, None, full.clone());
                        let adornments =
                            store.line_adornments(None, None, full.clone(), full_lines, |_| 0);
                        let inline = store.inline_adornments(None, None, full.clone());
                        let concealed = store.concealed_ranges(full);
                        black_box((spans.len(), adornments.len(), inline.len(), concealed.len()));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn insertion_undo_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotations_insertion_undo_recording");
    for &n in &[100usize, 1_000, 10_000] {
        // OLD: full snapshot clone + the shift, per keystroke.
        group.bench_with_input(BenchmarkId::new("snapshot_plus_shift", n), &n, |b, &n| {
            b.iter_batched(
                || populated_store(n),
                |mut store| {
                    for k in 0..50 {
                        let at = 5 + k;
                        let snap = store.snapshot();
                        black_box(snap.len());
                        store.on_edit(black_box(at), black_box(at), black_box(at + 1));
                    }
                    store
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.bench_with_input(BenchmarkId::new("shift_only", n), &n, |b, &n| {
            b.iter_batched(
                || populated_store(n),
                |mut store| {
                    for k in 0..50 {
                        let at = 5 + k;
                        store.on_edit(black_box(at), black_box(at), black_box(at + 1));
                    }
                    store
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn diagnostic_replace(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotations_diagnostic_replace");
    let n = 1_000usize;
    for &d in &[50usize, 500, 5_000] {
        group.bench_with_input(BenchmarkId::new("clear_then_loop", d), &d, |b, &d| {
            b.iter_batched(
                || {
                    let mut store = populated_store(n);
                    for i in 0..d {
                        store.create_diagnostic(i, 1, "old");
                    }
                    store
                },
                |mut store| {
                    store.clear_lsp_diagnostics();
                    for i in 0..d {
                        store.create_diagnostic(i, 2, "new");
                    }
                    black_box(store.query_at(0).count());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.bench_with_input(BenchmarkId::new("bulk_replace", d), &d, |b, &d| {
            b.iter_batched(
                || {
                    let mut store = populated_store(n);
                    for i in 0..d {
                        store.create_diagnostic(i, 1, "old");
                    }
                    store
                },
                |mut store| {
                    store.replace_lsp_diagnostics((0..d).map(|i| (i, 2, "new")));
                    black_box(store.query_at(0).count());
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    marker_shift_under_edits,
    query_after_invalidation,
    presentation_flatten,
    line_anchor_edit_tracking,
    render_viewport_scroll,
    render_full_document_scan,
    insertion_undo_recording,
    diagnostic_replace
);
criterion_main!(benches);
