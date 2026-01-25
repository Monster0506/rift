use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::buffer::TextBuffer;
use std::hint::black_box;

fn movement_semantic(c: &mut Criterion) {
    let mut group = c.benchmark_group("movement_semantic");

    let setup_text = || {
        let mut buf = TextBuffer::new(1024 * 1024).unwrap();
        // Create paragraph with many words
        let line = "word ".repeat(100) + "\n"; // 100 words per line
                                               // Create 100 paragraphs
        for _ in 0..100 {
            for _ in 0..10 {
                buf.insert_str(&line).unwrap();
            }
            buf.insert_char('\n').unwrap(); // Empty line for paragraph
        }
        buf
    };

    group.bench_function("move_word_right", |b| {
        b.iter_batched(
            setup_text,
            |mut buf| {
                // Move 1000 words
                for _ in 0..1000 {
                    black_box(buf.move_word_right());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("move_paragraph_forward", |b| {
        b.iter_batched(
            setup_text,
            |mut buf| {
                // Move 100 paragraphs
                for _ in 0..100 {
                    black_box(buf.move_paragraph_forward());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn movement_vertical(c: &mut Criterion) {
    let mut group = c.benchmark_group("movement_vertical");

    let setup_vertical = || {
        let mut buf = TextBuffer::new(10 * 1024).unwrap();
        // Create deep buffer with varying line lengths to stress column calc
        for i in 0..10_000 {
            let len = (i % 80) + 10;
            let line = "a".repeat(len) + "\n";
            buf.insert_str(&line).unwrap();
        }
        buf
    };

    group.bench_function("move_down_scan", |b| {
        b.iter_batched(
            setup_vertical,
            |mut buf| {
                // Move down 1000 lines
                // We set cursor to col 40 (middle) to force column matching logic
                buf.set_cursor(40).unwrap_or(());
                for _ in 0..1000 {
                    black_box(buf.move_down());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, movement_semantic, movement_vertical);
criterion_main!(benches);
