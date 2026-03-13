//! Performance instrumentation infrastructure.
//!
//! Enabled by the `perf_instrumentation` Cargo feature.  When the feature is
//! absent every [`perf_span!`] call is compiled away entirely — zero runtime
//! cost and no file I/O.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::{perf_span, perf::PerfFields};
//!
//! fn render_frame(rows: u32) {
//!     let _span = perf_span!(
//!         "render_frame",
//!         PerfFields { lines: Some(rows), ..Default::default() }
//!     );
//!     // … work …
//!     // Event is recorded automatically when `_span` is dropped.
//! }
//! ```
//!
//! Events are appended to `rift-perf.log` in the working directory and kept
//! in a fixed-size in-memory ring buffer (capacity [`RING_CAPACITY`]).  The
//! ring buffer can be read with [`recent_events`] or drained with
//! [`drain_events`] for a future `:perf` view.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Number of events kept in the in-memory ring buffer.
pub const RING_CAPACITY: usize = 256;

// ── public types ─────────────────────────────────────────────────────────────

/// Optional metadata attached to a [`PerfSpan`].
///
/// All fields default to `None`; use struct-update syntax to fill only what is
/// relevant:
///
/// ```rust,ignore
/// PerfFields { bytes: Some(n), ..Default::default() }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct PerfFields {
    /// Number of bytes involved in the operation.
    pub bytes: Option<u32>,
    /// Number of lines involved in the operation.
    pub lines: Option<u32>,
    /// A static label for additional context (e.g. `"full"`, `"incremental"`).
    pub tag: Option<&'static str>,
}

/// A completed performance measurement.
#[derive(Debug, Clone, Copy)]
pub struct PerfEvent {
    /// Name identifying the operation (e.g. `"render_frame"`).
    pub name: &'static str,
    /// Wall-clock elapsed time.
    pub duration: Duration,
    /// Optional metadata.
    pub fields: PerfFields,
}

/// An active timing span.  The event is recorded automatically when this value
/// is dropped, so early returns, `?`, and panics are all handled correctly.
pub struct PerfSpan {
    name: &'static str,
    start: Instant,
    fields: PerfFields,
}

impl PerfSpan {
    /// Start a new span.  Capture [`Instant::now`] immediately.
    #[inline]
    pub fn new(name: &'static str, fields: PerfFields) -> Self {
        Self {
            name,
            start: Instant::now(),
            fields,
        }
    }
}

impl Drop for PerfSpan {
    fn drop(&mut self) {
        record(PerfEvent {
            name: self.name,
            duration: self.start.elapsed(),
            fields: self.fields,
        });
    }
}

// ── global sink ───────────────────────────────────────────────────────────────

struct PerfSink {
    log: BufWriter<File>,
    ring: VecDeque<PerfEvent>,
}

/// `None` means the log file could not be opened; instrumentation is silently
/// disabled rather than panicking the editor.
static SINK: OnceLock<Mutex<Option<PerfSink>>> = OnceLock::new();

fn get_sink() -> &'static Mutex<Option<PerfSink>> {
    SINK.get_or_init(|| {
        let maybe = OpenOptions::new()
            .create(true)
            .append(true)
            .open("rift-perf.log")
            .ok()
            .map(|file| PerfSink {
                log: BufWriter::new(file),
                ring: VecDeque::with_capacity(RING_CAPACITY),
            });
        Mutex::new(maybe)
    })
}

// ── public API ────────────────────────────────────────────────────────────────

/// Record a [`PerfEvent`] into the ring buffer and append it to `rift-perf.log`.
///
/// This is called automatically by [`PerfSpan::drop`]; you rarely need to call
/// it directly.
#[inline]
pub fn record(event: PerfEvent) {
    if let Ok(mut guard) = get_sink().lock() {
        if let Some(sink) = guard.as_mut() {
            // Maintain the ring buffer.
            if sink.ring.len() >= RING_CAPACITY {
                sink.ring.pop_front();
            }
            sink.ring.push_back(event);

            // Write to log — no heap allocations in the hot path.
            let _ = write!(sink.log, "[perf] {}: {:?}", event.name, event.duration);
            if let Some(tag) = event.fields.tag {
                let _ = write!(sink.log, " tag={tag}");
            }
            if let Some(bytes) = event.fields.bytes {
                let _ = write!(sink.log, " bytes={bytes}");
            }
            if let Some(lines) = event.fields.lines {
                let _ = write!(sink.log, " lines={lines}");
            }
            let _ = writeln!(sink.log);
            let _ = sink.log.flush();
        }
    }
}

/// Return a snapshot of the most recent events without removing them.
pub fn recent_events() -> Vec<PerfEvent> {
    if let Ok(guard) = get_sink().lock() {
        if let Some(sink) = guard.as_ref() {
            return sink.ring.iter().copied().collect();
        }
    }
    Vec::new()
}

/// Drain all buffered events and return them in chronological order.
///
/// Intended for use by a future `:perf` view.
pub fn drain_events() -> Vec<PerfEvent> {
    if let Ok(mut guard) = get_sink().lock() {
        if let Some(sink) = guard.as_mut() {
            return sink.ring.drain(..).collect();
        }
    }
    Vec::new()
}
