//! Scripted replay: drive an `Editor` through a recorded operation
//! sequence for profiling and deterministic regression checks.

mod assert;
pub(crate) mod backend;
#[cfg(test)]
mod fuzz;
mod ops;
mod runner;
#[cfg(test)]
mod shrink;

#[cfg(test)]
pub(crate) use backend::ReplayBackend;
pub use ops::parse;
pub use runner::{run, RunReport};
