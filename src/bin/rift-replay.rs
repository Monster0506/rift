//! CLI runner for replay scripts: parses a script file, runs it against a
//! headless `Editor`, and prints marks, key latency, and perf spans.

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
            print_perf_events(mark);
        }
    }

    if let Some(p) = report.tick_percentiles() {
        println!(
            "keys: n={} avg={:.2?} p50={:.2?} p95={:.2?} max={:.2?}",
            report.ticks.len(),
            p.avg,
            p.p50,
            p.p95,
            p.max
        );
    }
}

#[cfg(feature = "perf_instrumentation")]
fn print_perf_events(mark: &replay::Mark) {
    for ev in &mark.perf_events {
        println!("      {:<24} {:>10.2?}", ev.name, ev.duration);
    }
}

#[cfg(not(feature = "perf_instrumentation"))]
fn print_perf_events(_mark: &replay::Mark) {}
