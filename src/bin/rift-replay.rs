//! CLI runner for replay scripts: parses a script file, runs it against a
//! headless `Editor`, and prints marks, key latency, and a perf breakdown.

use monster_rift::replay::{self, RunReport};
use std::time::Duration;

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: rift-replay <script-file>");
            std::process::exit(2);
        }
    };

    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("rift-replay: failed to read {path}: {e}");
        std::process::exit(1);
    });

    let ops = replay::parse(&source).unwrap_or_else(|e| {
        eprintln!("rift-replay: {path}: {e}");
        std::process::exit(1);
    });

    let report = replay::run(&ops, std::io::sink()).unwrap_or_else(|e| {
        eprintln!("rift-replay: {path}: [{}] {}", e.code, e.message);
        std::process::exit(1);
    });

    print_report(&report);
    print_frame_stats();
    print_span_alloc_stats();
}

#[cfg(feature = "perf_instrumentation")]
fn print_span_alloc_stats() {
    let mut stats = monster_rift::perf::span_stats();
    if stats.is_empty() {
        return;
    }
    stats.sort_by(|a, b| {
        let ta = a.1.avg_allocs * a.1.count as f64;
        let tb = b.1.avg_allocs * b.1.count as f64;
        tb.partial_cmp(&ta).unwrap_or(std::cmp::Ordering::Equal)
    });
    println!("\nper-span allocs (by total allocs):");
    println!(
        "  {:<28} {:>6} {:>10} {:>8} {:>8} {:>12} {:>10} {:>10}",
        "name", "n", "avg", "min", "max", "avg bytes", "avg dealloc", "avg clones"
    );
    for (name, s) in &stats {
        println!(
            "  {:<28} {:>6} {:>10.1} {:>8} {:>8} {:>12.0} {:>10.1} {:>10.1}",
            name,
            s.count,
            s.avg_allocs,
            s.min_allocs,
            s.max_allocs,
            s.avg_alloc_bytes,
            s.avg_deallocs,
            s.avg_clones
        );
    }
}

#[cfg(not(feature = "perf_instrumentation"))]
fn print_span_alloc_stats() {}

#[cfg(feature = "perf_instrumentation")]
fn print_frame_stats() {
    let s = monster_rift::perf::frame_stats();
    if s.count == 0 {
        return;
    }
    println!("\nframe stats (update_and_render, n={}):", s.count);
    println!(
        "  time:   avg={:.3}ms min={:.3}ms max={:.3}ms",
        s.avg_ms, s.min_ms, s.max_ms
    );
    println!(
        "  allocs: avg={:.1} min={} max={}",
        s.avg_allocs, s.min_allocs, s.max_allocs
    );
    println!(
        "  bytes:  avg={:.0} min={} max={}",
        s.avg_alloc_bytes, s.min_alloc_bytes, s.max_alloc_bytes
    );
    println!(
        "  deallocs: avg={:.1} min={} max={}",
        s.avg_deallocs, s.min_deallocs, s.max_deallocs
    );
    println!(
        "  dealloc bytes: avg={:.0} min={} max={}",
        s.avg_dealloc_bytes, s.min_dealloc_bytes, s.max_dealloc_bytes
    );
    println!(
        "  net allocs (alloc-dealloc): avg={:.1}",
        s.avg_allocs - s.avg_deallocs
    );
    println!(
        "  clones: avg={:.1} min={} max={}",
        s.avg_clones, s.min_clones, s.max_clones
    );
    let cursor_advances = monster_rift::perf::cursor_advance_count();
    println!(
        "  cursor advances (render decorators, whole run): total={} avg/frame={:.1}",
        cursor_advances,
        cursor_advances as f64 / s.count as f64
    );
    let (rows_painted, rows_skipped) = monster_rift::perf::row_paint_counts();
    let rows_total = rows_painted + rows_skipped;
    let skip_pct = if rows_total == 0 {
        0.0
    } else {
        rows_skipped as f64 / rows_total as f64 * 100.0
    };
    println!(
        "  content rows: painted={} blit-skipped={} ({:.1}% skipped)",
        rows_painted, rows_skipped, skip_pct
    );
}

#[cfg(not(feature = "perf_instrumentation"))]
fn print_frame_stats() {}

fn print_report(report: &RunReport) {
    if report.marks.is_empty() {
        println!("no marks recorded");
    } else {
        println!("marks:");
        let mut prev = Duration::ZERO;
        for mark in &report.marks {
            println!(
                "  {:<24} at {:>10.2?}  (+{:>10.2?})",
                mark.label,
                mark.at,
                mark.at.saturating_sub(prev)
            );
            prev = mark.at;
        }
    }

    if let Some(p) = report.tick_percentiles() {
        println!(
            "\nkeys: n={} avg={:.2?} p50={:.2?} p95={:.2?} max={:.2?}",
            report.ticks.len(),
            p.avg,
            p.p50,
            p.p95,
            p.max
        );
    }

    print_perf_summary(report);
}

#[cfg(feature = "perf_instrumentation")]
fn print_perf_summary(report: &RunReport) {
    let summary = report.perf_summary();
    if summary.is_empty() {
        return;
    }
    println!("\nperf spans (by total time):");
    println!(
        "  {:<20} {:>6} {:>10} {:>10} {:>10} {:>10}",
        "name", "n", "total", "avg", "p95", "max"
    );
    for s in &summary {
        println!(
            "  {:<20} {:>6} {:>10.2?} {:>10.2?} {:>10.2?} {:>10.2?}",
            s.name, s.count, s.total, s.avg, s.p95, s.max
        );
    }

    if let Some(total) = summary.iter().find(|s| s.name == "update_and_render") {
        let other: Duration = summary
            .iter()
            .filter(|s| s.name != "update_and_render")
            .map(|s| s.total)
            .sum();
        let unattributed = total.total.saturating_sub(other);
        let pct = if total.total.is_zero() {
            0.0
        } else {
            unattributed.as_secs_f64() / total.total.as_secs_f64() * 100.0
        };
        println!("\nunattributed: {unattributed:.2?} ({pct:.1}% of update_and_render)");
    }
}

#[cfg(not(feature = "perf_instrumentation"))]
fn print_perf_summary(_report: &RunReport) {}
