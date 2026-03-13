use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use monster_rift::buffer::line_index::LineIndex;
use monster_rift::buffer::rope::PieceTable;
use monster_rift::buffer::TextBuffer;
use monster_rift::character::Character;
use monster_rift::wrap::DisplayMap;
use std::hint::black_box;

fn make_buf(lines: usize, line_len: usize) -> TextBuffer {
    let mut buf = TextBuffer::new(lines * (line_len + 1)).unwrap();
    let line = "a".repeat(line_len) + "\n";
    for _ in 0..lines {
        buf.insert_str(&line).unwrap();
    }
    buf
}

fn make_buf_single_piece(lines: usize, line_len: usize) -> TextBuffer {
    let line = "a".repeat(line_len) + "\n";
    let content: Vec<Character> = line
        .chars()
        .map(Character::from)
        .cycle()
        .take(lines * (line_len + 1))
        .collect();
    let mut buf = TextBuffer::new(0).unwrap();
    buf.line_index = LineIndex { table: PieceTable::new(content) };
    buf
}

fn make_buf_long_lines(lines: usize, line_len: usize) -> TextBuffer {
    let mut buf = TextBuffer::new(lines * (line_len + 1)).unwrap();
    let line = "word ".repeat(line_len / 5) + "\n";
    for _ in 0..lines {
        buf.insert_str(&line).unwrap();
    }
    buf
}

fn bench_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("displaymap_build");

    for (lines, line_len) in [(100usize, 40usize), (1_000, 80), (10_000, 80)] {
        let buf = make_buf(lines, line_len);
        group.bench_with_input(
            BenchmarkId::new("short_lines", format!("{lines}x{line_len}")),
            &buf,
            |b, buf| {
                b.iter(|| black_box(DisplayMap::build(buf, 80, 4)));
            },
        );
    }

    for (lines, line_len) in [(100usize, 400usize), (1_000, 400), (5_000, 400)] {
        let buf = make_buf_long_lines(lines, line_len);
        group.bench_with_input(
            BenchmarkId::new("long_lines", format!("{lines}x{line_len}")),
            &buf,
            |b, buf| {
                b.iter(|| black_box(DisplayMap::build(buf, 80, 4)));
            },
        );
    }

    group.finish();
}

fn bench_build_single_piece(c: &mut Criterion) {
    let mut group = c.benchmark_group("displaymap_build_single_piece");

    for (lines, line_len) in [(100usize, 40usize), (1_000, 80), (10_000, 80)] {
        let buf = make_buf_single_piece(lines, line_len);
        group.bench_with_input(
            BenchmarkId::new("single_piece", format!("{lines}x{line_len}")),
            &buf,
            |b, buf| {
                b.iter(|| black_box(DisplayMap::build(buf, 80, 4)));
            },
        );
    }

    group.finish();
}

fn bench_redundant_builds(c: &mut Criterion) {
    let mut group = c.benchmark_group("displaymap_redundant");

    let buf_mp = make_buf(5_000, 80);
    group.bench_function("2x_build_many_piece", |b| {
        b.iter(|| {
            for _ in 0..2 {
                black_box(DisplayMap::build(&buf_mp, 80, 4));
            }
        });
    });

    let buf_sp = make_buf_single_piece(1_205, 61);
    group.bench_function("2x_build_single_piece_1205lines", |b| {
        b.iter(|| {
            for _ in 0..2 {
                black_box(DisplayMap::build(&buf_sp, 80, 4));
            }
        });
    });

    group.finish();
}

fn bench_nav(c: &mut Criterion) {
    let mut group = c.benchmark_group("displaymap_nav");

    let buf = make_buf_long_lines(1_000, 400);
    let dm = DisplayMap::build(&buf, 80, 4);
    let mid = buf.len() / 2;

    group.bench_function("visual_up_1000", |b| {
        b.iter(|| {
            let mut pos = mid;
            for _ in 0..1000 {
                pos = dm.visual_up(pos, &buf);
            }
            black_box(pos)
        });
    });

    group.bench_function("visual_down_1000", |b| {
        b.iter(|| {
            let mut pos = mid;
            for _ in 0..1000 {
                pos = dm.visual_down(pos, &buf);
            }
            black_box(pos)
        });
    });

    group.bench_function("char_to_visual_row_1000", |b| {
        b.iter(|| {
            for i in 0..1000usize {
                black_box(dm.char_to_visual_row(i % buf.len()));
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_build, bench_build_single_piece, bench_redundant_builds, bench_nav);
criterion_main!(benches);
