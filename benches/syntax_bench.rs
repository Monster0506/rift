use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::syntax::interval_tree::IntervalTree;
use std::hint::black_box;
use std::ops::Range;

fn interval_tree_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("interval_tree");

    // Pre-generate a large number of ranges to simulate syntax highlighting spans
    let range_count = 10_000;
    let max_val = 1_000_000;

    // Create random ranges (deterministic seed via calculation)
    let ranges: Vec<(Range<usize>, u32)> = (0..range_count)
        .map(|i| {
            let start = (i * 100) % max_val;
            let len = (i % 50) + 1;
            (start..(start + len), i as u32)
        })
        .collect();

    group.bench_function("build_tree", |b| {
        b.iter_batched(
            || ranges.clone(),
            |items| {
                black_box(IntervalTree::new(items));
            },
            criterion::BatchSize::LargeInput,
        )
    });

    group.bench_function("query_tree", |b| {
        let tree = IntervalTree::new(ranges.clone());
        let query_len = 100; // Viewing a standard screenful
        b.iter(|| {
            // query roughly near the middle
            black_box(tree.query(500_000..(500_000 + query_len)));
        })
    });

    group.finish();
}

criterion_group!(benches, interval_tree_bench);
criterion_main!(benches);
