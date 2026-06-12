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

fn marker_shift_under_edits(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotations_marker_shift");
    for &n in &[100usize, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            // Each iteration inserts one byte near the front, forcing nearly
            // every marker after it to shift. 50 edits approximates a burst of
            // typing at the top of a heavily annotated buffer.
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
        // Point queries spread across the buffer, each forcing one index rebuild
        // because the preceding edit invalidated it.
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

        // Range (viewport-style) queries, same invalidation pattern.
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

        // next_interactive walks the interactive subset; check it scales.
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
            // A store where every annotation is styled, so flattening must sort
            // and merge `n` overlapping-capable spans (the render hot path).
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
                let spans = store.presentation_spans(black_box(None), black_box(None));
                black_box(spans.len())
            })
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    marker_shift_under_edits,
    query_after_invalidation,
    presentation_flatten
);
criterion_main!(benches);
