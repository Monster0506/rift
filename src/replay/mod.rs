//! Scripted replay: drive an `Editor` through a recorded operation
//! sequence for profiling and deterministic regression checks.

mod assert;
pub(crate) mod backend;
#[cfg(test)]
pub(crate) mod fuzz;
pub(crate) mod ops;
pub(crate) mod runner;
#[cfg(test)]
pub(crate) mod shrink;

#[cfg(test)]
pub(crate) use backend::ReplayBackend;
pub use ops::parse;
pub use runner::{run, RunReport};
