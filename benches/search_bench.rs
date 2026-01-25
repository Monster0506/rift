use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::buffer::TextBuffer;
use monster_rift::search::{compile_regex, find_all};
use std::hint::black_box;

fn search_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_compilation");

    group.bench_function("compile_literal", |b| {
        b.iter(|| black_box(compile_regex("simple_literal")))
    });

    group.bench_function("compile_regex_simple", |b| {
        b.iter(|| black_box(compile_regex(r"\w+\s+\d+")))
    });

    group.bench_function("compile_regex_complex", |b| {
        // A more complex regex that might trigger different paths
        b.iter(|| {
            black_box(compile_regex(
                r"(?i)^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$",
            ))
        })
    });

    group.finish();
}

fn search_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_execution");

    let setup_text = || {
        let mut buf = TextBuffer::new(1024 * 1024).unwrap();
        // 1000 iterations of a pattern
        for i in 0..1000 {
            buf.insert_str(&format!("Prefix match_{} Suffix\n", i))
                .unwrap();
        }
        // Fill some noise
        for _ in 0..1000 {
            buf.insert_str("Calculon is a acting robot who is very dramatic.\n")
                .unwrap();
        }
        buf
    };

    group.bench_function("find_all_literal", |b| {
        let buf = setup_text();
        b.iter(|| {
            black_box(find_all(&buf, "match_500").unwrap());
        })
    });

    group.bench_function("find_all_regex_anchored", |b| {
        let buf = setup_text();
        // Regex for "match_" followed by digits at start of line (conceptually, though our formatting puts Prefix first)
        // Let's search for "Prefix match_\d+"
        b.iter(|| {
            black_box(find_all(&buf, r"match_\d+").unwrap());
        })
    });

    group.bench_function("find_all_regex_complex", |b| {
        let buf = setup_text();
        // Case insensitive search for "calculon"
        b.iter(|| {
            black_box(find_all(&buf, "(?i)calculon").unwrap());
        })
    });

    group.finish();
}

criterion_group!(benches, search_compilation, search_execution);
criterion_main!(benches);
