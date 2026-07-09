//! Scripted replay: drive an `Editor` through a recorded operation
//! sequence for profiling and deterministic regression checks.

pub mod ops;

pub use ops::{parse, Assertion, ParseError, ScriptOp};
