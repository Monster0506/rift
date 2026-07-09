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
}

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
}

#[cfg(not(feature = "perf_instrumentation"))]
fn print_perf_summary(_report: &RunReport) {}
