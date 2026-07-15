//! Performance instrumentation, gated by the `perf_instrumentation` Cargo
//! feature (zero-cost no-op when absent). Events go to `rift-perf.log` and a ring buffer.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Number of events kept in the in-memory ring buffer.
pub const RING_CAPACITY: usize = 16384;

/// Optional metadata attached to a [`PerfSpan`]; all fields default to `None`,
/// filled via struct-update syntax (`PerfFields { bytes: Some(n), ..Default::default() }`).
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
    /// Allocations performed between span start and drop.
    pub allocs: u64,
    /// Bytes allocated between span start and drop.
    pub alloc_bytes: u64,
    /// Deallocations performed between span start and drop.
    pub deallocs: u64,
    /// Bytes freed between span start and drop.
    pub dealloc_bytes: u64,
    /// `perf_clone!`-instrumented clones performed between span start and drop.
    pub clones: u64,
    /// Optional metadata.
    pub fields: PerfFields,
}

/// An active timing span.  The event is recorded automatically when this value
/// is dropped, so early returns, `?`, and panics are all handled correctly.
pub struct PerfSpan {
    name: &'static str,
    start: Instant,
    start_allocs: u64,
    start_alloc_bytes: u64,
    start_deallocs: u64,
    start_dealloc_bytes: u64,
    start_clones: u64,
    fields: PerfFields,
}

impl PerfSpan {
    /// Start a new span.  Capture [`Instant::now`] and the current
    /// alloc/dealloc/clone counters immediately.
    #[inline]
    pub fn new(name: &'static str, fields: PerfFields) -> Self {
        Self {
            name,
            start: Instant::now(),
            start_allocs: ALLOC_COUNT.load(Ordering::Relaxed),
            start_alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            start_deallocs: DEALLOC_COUNT.load(Ordering::Relaxed),
            start_dealloc_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
            start_clones: CLONE_COUNT.load(Ordering::Relaxed),
            fields,
        }
    }
}

impl Drop for PerfSpan {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        let allocs = ALLOC_COUNT
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_allocs);
        let alloc_bytes = ALLOC_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_alloc_bytes);
        let deallocs = DEALLOC_COUNT
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_deallocs);
        let dealloc_bytes = DEALLOC_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_dealloc_bytes);
        let clones = CLONE_COUNT
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_clones);

        fold_into_named_aggregate(
            self.name,
            duration,
            allocs,
            alloc_bytes,
            deallocs,
            dealloc_bytes,
            clones,
        );
        record(PerfEvent {
            name: self.name,
            duration,
            allocs,
            alloc_bytes,
            deallocs,
            dealloc_bytes,
            clones,
            fields: self.fields,
        });
    }
}

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

/// Record a [`PerfEvent`] into the ring buffer and `rift-perf.log`. Called
/// automatically by [`PerfSpan::drop`]; rarely needed directly.
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
            // No per-event flush: a syscall per span drop dominated frame
            // cost once span density rose. BufWriter flushes on Drop.
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

/// Drain all buffered events in chronological order, for a future `:perf` view.
pub fn drain_events() -> Vec<PerfEvent> {
    if let Ok(mut guard) = get_sink().lock() {
        if let Some(sink) = guard.as_mut() {
            return sink.ring.drain(..).collect();
        }
    }
    Vec::new()
}

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

/// Thin wrapper around [`System`] that atomically counts every alloc/dealloc.
/// Installed as `#[global_allocator]` below, so only builds with this feature pay for it.
pub struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

/// Number of clones performed at hot-path call sites instrumented with
/// [`crate::perf_clone!`].
static CLONE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Record one clone at an instrumented call site. Called by [`crate::perf_clone!`];
/// rarely useful to call directly.
#[inline]
pub fn count_clone() {
    CLONE_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Fast-forward cursor advance steps across render decorators - deterministic
/// and immune to wall-clock noise, unlike timing spans.
static CURSOR_ADVANCE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Record one fast-forward cursor advance step. Called by
/// [`crate::perf_cursor_advance!`]; rarely useful to call directly.
#[inline]
pub fn count_cursor_advance() {
    CURSOR_ADVANCE_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Total fast-forward cursor advance steps recorded so far in this process.
pub fn cursor_advance_count() -> u64 {
    CURSOR_ADVANCE_COUNT.load(Ordering::Relaxed)
}

/// Visible content rows painted (full pipeline) vs. skipped (dirty-row
/// blit) - deterministic, immune to wall-clock noise.
static ROWS_PAINTED: AtomicU64 = AtomicU64::new(0);
static ROWS_BLIT_SKIPPED: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn count_row_painted() {
    ROWS_PAINTED.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn count_row_blit_skipped() {
    ROWS_BLIT_SKIPPED.fetch_add(1, Ordering::Relaxed);
}

/// (rows painted, rows skipped via scroll blit) recorded so far in this process.
pub fn row_paint_counts() -> (u64, u64) {
    (
        ROWS_PAINTED.load(Ordering::Relaxed),
        ROWS_BLIT_SKIPPED.load(Ordering::Relaxed),
    )
}

#[derive(Debug, Clone, Copy)]
struct Aggregate {
    count: u64,
    sum_ns: u64,
    min_ns: u64,
    max_ns: u64,
    sum_allocs: u64,
    min_allocs: u64,
    max_allocs: u64,
    sum_alloc_bytes: u64,
    min_alloc_bytes: u64,
    max_alloc_bytes: u64,
    sum_deallocs: u64,
    min_deallocs: u64,
    max_deallocs: u64,
    sum_dealloc_bytes: u64,
    min_dealloc_bytes: u64,
    max_dealloc_bytes: u64,
    sum_clones: u64,
    min_clones: u64,
    max_clones: u64,
}

impl Aggregate {
    const fn new() -> Self {
        Self {
            count: 0,
            sum_ns: 0,
            min_ns: u64::MAX,
            max_ns: 0,
            sum_allocs: 0,
            min_allocs: u64::MAX,
            max_allocs: 0,
            sum_alloc_bytes: 0,
            min_alloc_bytes: u64::MAX,
            max_alloc_bytes: 0,
            sum_deallocs: 0,
            min_deallocs: u64::MAX,
            max_deallocs: 0,
            sum_dealloc_bytes: 0,
            min_dealloc_bytes: u64::MAX,
            max_dealloc_bytes: 0,
            sum_clones: 0,
            min_clones: u64::MAX,
            max_clones: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fold(
        &mut self,
        duration: Duration,
        allocs: u64,
        alloc_bytes: u64,
        deallocs: u64,
        dealloc_bytes: u64,
        clones: u64,
    ) {
        let ns = duration.as_nanos() as u64;
        self.count += 1;
        self.sum_ns += ns;
        self.min_ns = self.min_ns.min(ns);
        self.max_ns = self.max_ns.max(ns);
        self.sum_allocs += allocs;
        self.min_allocs = self.min_allocs.min(allocs);
        self.max_allocs = self.max_allocs.max(allocs);
        self.sum_alloc_bytes += alloc_bytes;
        self.min_alloc_bytes = self.min_alloc_bytes.min(alloc_bytes);
        self.max_alloc_bytes = self.max_alloc_bytes.max(alloc_bytes);
        self.sum_deallocs += deallocs;
        self.min_deallocs = self.min_deallocs.min(deallocs);
        self.max_deallocs = self.max_deallocs.max(deallocs);
        self.sum_dealloc_bytes += dealloc_bytes;
        self.min_dealloc_bytes = self.min_dealloc_bytes.min(dealloc_bytes);
        self.max_dealloc_bytes = self.max_dealloc_bytes.max(dealloc_bytes);
        self.sum_clones += clones;
        self.min_clones = self.min_clones.min(clones);
        self.max_clones = self.max_clones.max(clones);
    }

    fn to_frame_stats(self) -> FrameStats {
        if self.count == 0 {
            return FrameStats::default();
        }
        let count = self.count as f64;
        FrameStats {
            count: self.count,
            avg_ms: (self.sum_ns as f64 / count) / 1_000_000.0,
            min_ms: self.min_ns as f64 / 1_000_000.0,
            max_ms: self.max_ns as f64 / 1_000_000.0,
            avg_allocs: self.sum_allocs as f64 / count,
            min_allocs: self.min_allocs,
            max_allocs: self.max_allocs,
            avg_alloc_bytes: self.sum_alloc_bytes as f64 / count,
            min_alloc_bytes: self.min_alloc_bytes,
            max_alloc_bytes: self.max_alloc_bytes,
            avg_deallocs: self.sum_deallocs as f64 / count,
            min_deallocs: self.min_deallocs,
            max_deallocs: self.max_deallocs,
            avg_dealloc_bytes: self.sum_dealloc_bytes as f64 / count,
            min_dealloc_bytes: self.min_dealloc_bytes,
            max_dealloc_bytes: self.max_dealloc_bytes,
            avg_clones: self.sum_clones as f64 / count,
            min_clones: self.min_clones,
            max_clones: self.max_clones,
        }
    }
}

/// Per-span-name accumulation, read via [`span_stats`]. Keyed by the same
/// `&'static str` names passed to [`crate::perf_span!`].
static SPAN_AGGREGATES: OnceLock<Mutex<std::collections::HashMap<&'static str, Aggregate>>> =
    OnceLock::new();

fn get_span_aggregates() -> &'static Mutex<std::collections::HashMap<&'static str, Aggregate>> {
    SPAN_AGGREGATES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

#[allow(clippy::too_many_arguments)]
fn fold_into_named_aggregate(
    name: &'static str,
    duration: Duration,
    allocs: u64,
    alloc_bytes: u64,
    deallocs: u64,
    dealloc_bytes: u64,
    clones: u64,
) {
    if let Ok(mut map) = get_span_aggregates().lock() {
        map.entry(name).or_insert_with(Aggregate::new).fold(
            duration,
            allocs,
            alloc_bytes,
            deallocs,
            dealloc_bytes,
            clones,
        );
    }
}

/// Per-span-name time/alloc/clone breakdown, one entry per distinct
/// `perf_span!`/frame name seen so far. Unordered.
pub fn span_stats() -> Vec<(&'static str, FrameStats)> {
    let map = match get_span_aggregates().lock() {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };
    map.iter()
        .map(|(name, agg)| (*name, agg.to_frame_stats()))
        .collect()
}

/// Session-level accumulation across every `update_and_render` call, read via
/// [`frame_stats`].
static AGGREGATE: Mutex<Aggregate> = Mutex::new(Aggregate::new());

/// Aggregated per-frame render statistics for the life of the process, read
/// via [`frame_stats`]. All fields are zero before the first frame completes.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub count: u64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub avg_allocs: f64,
    pub min_allocs: u64,
    pub max_allocs: u64,
    pub avg_alloc_bytes: f64,
    pub min_alloc_bytes: u64,
    pub max_alloc_bytes: u64,
    pub avg_deallocs: f64,
    pub min_deallocs: u64,
    pub max_deallocs: u64,
    pub avg_dealloc_bytes: f64,
    pub min_dealloc_bytes: u64,
    pub max_dealloc_bytes: u64,
    pub avg_clones: f64,
    pub min_clones: u64,
    pub max_clones: u64,
}

/// RAII guard for one `update_and_render` call: folds wall-clock/alloc/dealloc/
/// clone deltas into the session [`AGGREGATE`] on drop, so early returns can't skip it.
pub struct FrameGuard {
    start: Instant,
    start_allocs: u64,
    start_alloc_bytes: u64,
    start_deallocs: u64,
    start_dealloc_bytes: u64,
    start_clones: u64,
}

/// Begin timing one `update_and_render` call. The returned guard finalizes
/// automatically when dropped at the end of the frame.
#[inline]
pub fn begin_frame() -> FrameGuard {
    FrameGuard {
        start: Instant::now(),
        start_allocs: ALLOC_COUNT.load(Ordering::Relaxed),
        start_alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
        start_deallocs: DEALLOC_COUNT.load(Ordering::Relaxed),
        start_dealloc_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
        start_clones: CLONE_COUNT.load(Ordering::Relaxed),
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        let allocs = ALLOC_COUNT
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_allocs);
        let alloc_bytes = ALLOC_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_alloc_bytes);
        let deallocs = DEALLOC_COUNT
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_deallocs);
        let dealloc_bytes = DEALLOC_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_dealloc_bytes);
        let clones = CLONE_COUNT
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_clones);

        if let Ok(mut agg) = AGGREGATE.lock() {
            agg.fold(
                duration,
                allocs,
                alloc_bytes,
                deallocs,
                dealloc_bytes,
                clones,
            );
        }
        fold_into_named_aggregate(
            "update_and_render",
            duration,
            allocs,
            alloc_bytes,
            deallocs,
            dealloc_bytes,
            clones,
        );

        record(PerfEvent {
            name: "update_and_render",
            duration,
            allocs,
            alloc_bytes,
            deallocs,
            dealloc_bytes,
            clones,
            fields: PerfFields::default(),
        });
    }
}

/// Read the current session-level frame statistics accumulated by [`FrameGuard`].
/// Returns all-zero stats if no frame has completed yet.
pub fn frame_stats() -> FrameStats {
    let agg = match AGGREGATE.lock() {
        Ok(g) => *g,
        Err(_) => return FrameStats::default(),
    };
    agg.to_frame_stats()
}
