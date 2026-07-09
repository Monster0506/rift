//! Scripted replay: drive an `Editor` through a recorded operation
//! sequence for profiling and deterministic regression checks.

mod assert;
pub mod backend;
pub mod ops;
pub mod runner;

pub use backend::ReplayBackend;
pub use ops::{parse, Assertion, ParseError, ScriptOp};
#[cfg(feature = "perf_instrumentation")]
pub use runner::PerfSpanSummary;
pub use runner::{run, Mark, Percentiles, RunReport, TickTiming};
