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

/// Line/position lookups on a single-piece buffer (a freshly-opened file) at increasing depth. If these scan newlines within the one big piece,
fn buffer_line_lookup_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_line_lookup_depth");

    // ~5000 lines, ~60 chars each, loaded as one piece (no edits).
    let line = "the quick brown fox jumps over the lazy dog and runs away.\n";
    let content: String = line.repeat(5_000);
    let total = content.chars().count();

    let mut buf = TextBuffer::new(content.len() + 16).unwrap();
    buf.insert_str(&content).unwrap();

    for &frac in &[1usize, 25, 50, 75, 99] {
        let pos = total * frac / 100;
        let line_no = buf.line_index.get_line_at(pos);
        group.bench_function(format!("get_line_at_{frac}pct"), |b| {
            b.iter(|| black_box(buf.line_index.get_line_at(black_box(pos))));
        });
        group.bench_function(format!("get_start_line_{frac}pct"), |b| {
            b.iter(|| black_box(buf.line_index.get_start(black_box(line_no))));
        });
    }

    group.finish();
}

/// Edit latency on a large buffer with the line cache "warm"
fn buffer_edit_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_edit_latency");

    let line = "the quick brown fox jumps over the lazy dog and runs away.\n";
    let content: String = line.repeat(5_000);
    let total = content.chars().count();
    let mid = total / 2;

    let make = || {
        let mut buf = TextBuffer::new(content.len() + 4096).unwrap();
        buf.insert_str(&content).unwrap();
        // Warm any line cache the way a render frame would.
        let _ = buf.line_index.get_line_at(mid);
        let _ = buf.line_index.get_start(buf.line_index.get_line_at(mid));
        buf
    };

    group.bench_function("insert_char_mid", |b| {
        b.iter_batched(
            make,
            |mut buf| {
                buf.set_cursor(mid).ok();
                for _ in 0..50 {
                    buf.insert_char(black_box('z')).unwrap();
                }
                buf
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("insert_newline_mid", |b| {
        b.iter_batched(
            make,
            |mut buf| {
                buf.set_cursor(mid).ok();
                for _ in 0..50 {
                    buf.insert_char(black_box('\n')).unwrap();
                }
                buf
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("delete_char_mid", |b| {
        b.iter_batched(
            make,
            |mut buf| {
                for _ in 0..50 {
                    buf.delete_range(black_box(mid), 1);
                }
                buf
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    benches,
    buffer_insertion,
    buffer_deletion,
    buffer_access,
    buffer_line_lookup_depth,
    buffer_edit_latency
);
criterion_main!(benches);
