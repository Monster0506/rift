//! Job modules. Import `Job` and `JobPayload` from here in any test file,
//! including nested test modules.
pub(crate) mod completion;
pub(crate) mod explorer;
pub(crate) mod explorer_preview;
pub(crate) mod file_operations;
pub(crate) mod fs;
pub(crate) mod git;
#[cfg(feature = "treesitter")]
pub(crate) mod syntax;
#[cfg(feature = "terminal_emulation")]
pub(crate) mod terminal_job;
pub(crate) mod undotree;

#[cfg(test)]
#[path = "test_support.rs"]
pub(crate) mod test_support;
