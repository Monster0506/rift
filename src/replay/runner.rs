//! Executes parsed [`ScriptOp`]s against a real `Editor`, one `tick()` per
//! scripted key, recording named timing marks along the way.

use super::backend::ReplayBackend;
use super::ops::ScriptOp;
use crate::editor::Editor;
use crate::error::{ErrorType, RiftError};
use crate::key::Key;
use std::io::Write;
use std::time::{Duration, Instant};

/// A named timing checkpoint, in the order it was hit.
#[derive(Debug, Clone)]
pub struct Mark {
    pub label: String,
    pub at: Duration,
    /// Perf spans recorded since the previous mark (or session start).
    /// Only populated when built with the `perf_instrumentation` feature.
    #[cfg(feature = "perf_instrumentation")]
    pub perf_events: Vec<crate::perf::PerfEvent>,
}

/// Wall-clock cost of a single `tick()` spent processing one scripted key.
#[derive(Debug, Clone, Copy)]
pub struct TickTiming {
    pub key: Key,
    pub duration: Duration,
}

/// avg/p50/p95/max over a set of durations.
#[derive(Debug, Clone, Copy)]
pub struct Percentiles {
    pub avg: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub max: Duration,
}

/// Total cost of one named perf span, aggregated across every mark.
#[cfg(feature = "perf_instrumentation")]
#[derive(Debug, Clone, Copy)]
pub struct PerfSpanSummary {
    pub name: &'static str,
    pub count: usize,
    pub total: Duration,
    pub avg: Duration,
    pub p95: Duration,
    pub max: Duration,
}

/// Outcome of running a script to completion.
#[derive(Debug, Clone, Default)]
pub struct RunReport {
    pub marks: Vec<Mark>,
    pub ticks: Vec<TickTiming>,
}

impl RunReport {
    /// Latency percentiles across every scripted keypress, or `None` if the
    /// script never typed anything.
    pub fn tick_percentiles(&self) -> Option<Percentiles> {
        if self.ticks.is_empty() {
            return None;
        }
        let mut sorted: Vec<Duration> = self.ticks.iter().map(|t| t.duration).collect();
        sorted.sort();
        let n = sorted.len();
        let avg = sorted.iter().sum::<Duration>() / n as u32;
        Some(Percentiles {
            avg,
            p50: sorted[n / 2],
            p95: sorted[(n * 95 / 100).min(n - 1)],
            max: sorted[n - 1],
        })
    }

    /// Perf spans from every mark, grouped by name and sorted by total time
    /// descending, biggest contributors to the run first.
    #[cfg(feature = "perf_instrumentation")]
    pub fn perf_summary(&self) -> Vec<PerfSpanSummary> {
        use std::collections::HashMap;

        let mut by_name: HashMap<&'static str, Vec<Duration>> = HashMap::new();
        for mark in &self.marks {
            for ev in &mark.perf_events {
                by_name.entry(ev.name).or_default().push(ev.duration);
            }
        }

        let mut summary: Vec<PerfSpanSummary> = by_name
            .into_iter()
            .map(|(name, mut durations)| {
                durations.sort();
                let n = durations.len();
                let total: Duration = durations.iter().sum();
                PerfSpanSummary {
                    name,
                    count: n,
                    total,
                    avg: total / n as u32,
                    p95: durations[(n * 95 / 100).min(n - 1)],
                    max: durations[n - 1],
                }
            })
            .collect();
        summary.sort_by(|a, b| b.total.cmp(&a.total));
        summary
    }
}

const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;
const IDLE_POLL_MS: u64 = 5;

/// Run `ops` against a fresh headless `Editor`, encoding output through
/// `writer` via the real `CrosstermBackend` serialization path.
pub fn run<W: Write>(ops: &[ScriptOp], writer: W) -> Result<RunReport, RiftError> {
    let mut rows = DEFAULT_ROWS;
    let mut cols = DEFAULT_COLS;
    let mut open_path: Option<String> = None;
    let mut writer = Some(writer);
    let mut editor: Option<Editor<ReplayBackend<W>>> = None;
    let mut report = RunReport::default();
    let clock = Instant::now();

    for op in ops {
        if editor.is_none() {
            match op {
                ScriptOp::Size { rows: r, cols: c } => {
                    rows = *r;
                    cols = *c;
                    continue;
                }
                ScriptOp::Open(path) => {
                    open_path = Some(path.clone());
                    continue;
                }
                ScriptOp::New => {
                    open_path = None;
                    continue;
                }
                _ => start(&mut editor, &mut writer, &mut open_path, rows, cols)?,
            }
        }
        let ed = editor.as_mut().expect("session started above");

        match op {
            ScriptOp::Size { .. } | ScriptOp::Open(_) | ScriptOp::New => {
                return Err(order_error(
                    "open/new/size must precede the first keys/wait/mark/assert",
                ));
            }
            ScriptOp::Keys(keys) => {
                ed.term.push_keys(keys.iter().copied());
                for key in keys {
                    let started = Instant::now();
                    ed.tick()?;
                    report.ticks.push(TickTiming {
                        key: *key,
                        duration: started.elapsed(),
                    });
                }
            }
            ScriptOp::WaitIdle { timeout_ms } => {
                wait_idle(ed, Duration::from_millis(*timeout_ms))?;
            }
            ScriptOp::Mark(label) => {
                report.marks.push(Mark {
                    label: label.clone(),
                    at: clock.elapsed(),
                    #[cfg(feature = "perf_instrumentation")]
                    perf_events: crate::perf::drain_events(),
                });
            }
            ScriptOp::Assert(assertion) => {
                super::assert::check(ed, assertion)?;
            }
        }
    }

    // A script that's only open/new/size (no behavior) still starts the
    // session, so a bare "does this file open" check is a valid script.
    start(&mut editor, &mut writer, &mut open_path, rows, cols)?;

    Ok(report)
}

fn start<W: Write>(
    editor: &mut Option<Editor<ReplayBackend<W>>>,
    writer: &mut Option<W>,
    open_path: &mut Option<String>,
    rows: u16,
    cols: u16,
) -> Result<(), RiftError> {
    if editor.is_some() {
        return Ok(());
    }
    let writer = writer.take().expect("writer consumed at most once");
    let backend = ReplayBackend::new(writer, rows, cols);
    *editor = Some(Editor::with_file(backend, open_path.take())?);
    Ok(())
}

/// Ticks for up to `timeout`, giving queued job/LSP/plugin work a chance to
/// land via the same idle path the live event loop uses between keys.
fn wait_idle<W: Write>(
    ed: &mut Editor<ReplayBackend<W>>,
    timeout: Duration,
) -> Result<(), RiftError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        ed.tick()?;
        std::thread::sleep(Duration::from_millis(IDLE_POLL_MS));
    }
    Ok(())
}

fn order_error(message: &str) -> RiftError {
    RiftError::new(ErrorType::Execution, "REPLAY_ORDER", message)
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
