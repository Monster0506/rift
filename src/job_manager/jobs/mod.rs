//! Job modules
//!
//! # Usage
//!
//! Import `Job` and `JobPayload` in any test file (including nested test modules):
pub mod cache_warming;
pub mod completion;
pub mod explorer;
pub mod explorer_preview;
pub mod file_operations;
pub mod fs;
#[cfg(feature = "treesitter")]
pub mod syntax;
pub mod terminal_job;
pub mod undotree;

#[cfg(test)]
pub(crate) mod test_support {
    use crate::job_manager::{JobMessage, JobPayload};
    use std::sync::mpsc::Receiver;

    /// The first `Custom` payload of type `T` received, downcast and unboxed.
    pub(crate) fn recv_custom_payload<T: JobPayload>(rx: &Receiver<JobMessage>) -> Option<Box<T>> {
        rx.try_iter().find_map(|m| match m {
            JobMessage::Custom(_, payload) => payload.into_any().downcast::<T>().ok(),
            _ => None,
        })
    }
}
