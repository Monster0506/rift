//! Scripted replay: drive an `Editor` through a recorded operation
//! sequence for profiling and deterministic regression checks.

pub mod backend;
pub mod ops;
pub mod runner;

pub use backend::ReplayBackend;
pub use ops::{parse, Assertion, ParseError, ScriptOp};
pub use runner::{run, Mark, RunReport};
