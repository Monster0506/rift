//! Job modules. Import `Job` and `JobPayload` from here in any test file,
//! including nested test modules.
pub use crate::job_manager::{AsyncOpDomain, AsyncToken};
pub mod cache_warming;
pub mod completion;
pub mod explorer;
pub mod explorer_preview;
pub mod file_operations;
pub mod fs;
pub mod git;
#[cfg(feature = "treesitter")]
pub mod syntax;
#[cfg(feature = "terminal_emulation")]
pub mod terminal_job;
pub mod undotree;

#[cfg(test)]
#[path = "test_support.rs"]
pub(crate) mod test_support;
