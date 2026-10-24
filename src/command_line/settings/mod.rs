//! Settings registry
//! Configuration-driven registry for :set command options

pub(crate) mod definitions;
pub(crate) mod descriptor;
pub(crate) mod registry;
pub(crate) mod resolver;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
