//! Scripted replay: drive an `Editor` through a recorded operation
//! sequence for profiling and deterministic regression checks.

mod assert;
pub(crate) mod backend;
#[cfg(test)]
mod fuzz;
pub(crate) mod ops;
pub(crate) mod runner;
#[cfg(test)]
mod shrink;
