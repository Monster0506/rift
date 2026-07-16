use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::character::Character;
use monster_rift::layer::Cell;
use monster_rift::screen_buffer::DoubleBuffer;
use std::hint::black_box;

fn screen_diffing(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_diffing");

    let rows = 40;
    let cols = 120;

    let setup_buffer = || DoubleBuffer::new(rows, cols);

    group.bench_function("diff_full_change", |b| {
        b.iter_batched(
            || {
                let mut buf = setup_buffer();
                // Make every cell different
                for r in 0..rows {
                    for c in 0..cols {
                        buf.set_cell(r, c, Cell::new(Character::from('a')));
                    }
                }
                buf
            },
            |mut buf| {
                // Compute batches (the expensive part of diffing)
                black_box(buf.get_batched_changes());
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("diff_no_change", |b| {
        b.iter_batched(
            || {
                let mut buf = setup_buffer();
                // swap() is the only public way to clear force_redraw without
                // a full render_to_terminal call.
                buf.swap();
                buf
            },
            |mut buf| {
                black_box(buf.get_batched_changes());
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn screen_updates(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_updates");

    let rows = 40;
    let cols = 120;

    group.bench_function("set_cell_random", |b| {
        let mut buf = DoubleBuffer::new(rows, cols);
        let cell = Cell::new(Character::from('x'));
        let mut i = 0;
        b.iter(|| {
            i = (i + 1) % (rows * cols);
            let r = i / cols;
            let col = i % cols;
            black_box(buf.set_cell(r, col, cell.clone()));
        })
    });

    group.finish();
}

criterion_group!(benches, screen_diffing, screen_updates);
criterion_main!(benches);
