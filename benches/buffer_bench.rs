use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;

use monster_rift::buffer::TextBuffer;

fn buffer_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_insertion");

    // Benchmark single char insertion at end
    group.bench_function("insert_char_end", |b| {
        b.iter_batched(
            || TextBuffer::new(1024).unwrap(),
            |mut buf| {
                for _ in 0..100 {
                    buf.insert_char(black_box('a')).unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // Benchmark string insertion
    static TEXT: &str = "The quick brown fox jumps over the lazy dog. ";
    group.throughput(Throughput::Bytes(TEXT.len() as u64));
    group.bench_function("insert_str_small", |b| {
        b.iter_batched(
            || TextBuffer::new(1024).unwrap(),
            |mut buf| {
                buf.insert_str(black_box(TEXT)).unwrap();
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn buffer_deletion(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_deletion");

    // Setup a buffer with some content
    let setup_buf = || {
        let mut buf = TextBuffer::new(1024).unwrap();
        for _ in 0..100 {
            buf.insert_str("Some text to delete. ").unwrap();
        }
        buf
    };

    group.bench_function("delete_backward", |b| {
        b.iter_batched(
            setup_buf,
            |mut buf| {
                // Delete 50 chars
                for _ in 0..50 {
                    buf.delete_backward();
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn buffer_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_access");

    // Large buffer setup
    let setup_large_buf = || {
        let mut buf = TextBuffer::new(1024 * 1024).unwrap();
        let line = "This is a line of text for testing buffer access speeds.\n";
        for _ in 0..10_000 {
            buf.insert_str(line).unwrap();
        }
        buf
    };

    group.bench_function("iter_full", |b| {
        let buf = setup_large_buf();
        b.iter(|| {
            for c in buf.iter() {
                black_box(c);
            }
        })
    });

    group.bench_function("get_line_bytes_random", |b| {
        let buf = setup_large_buf();
        let total_lines = buf.get_total_lines();
        let mut i = 0;
        b.iter(|| {
            // Pseudo-random access
            i = (i + 13) % total_lines;
            black_box(buf.get_line_bytes(i));
        })
    });

    group.finish();
}

criterion_group!(benches, buffer_insertion, buffer_deletion, buffer_access);
criterion_main!(benches);
