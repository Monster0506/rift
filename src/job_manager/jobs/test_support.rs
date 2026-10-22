use crate::job_manager::{JobMessage, JobPayload};
use std::sync::mpsc::Receiver;

/// The first `Custom` payload of type `T` received, downcast and unboxed.
pub(crate) fn recv_custom_payload<T: JobPayload>(rx: &Receiver<JobMessage>) -> Option<Box<T>> {
    rx.try_iter().find_map(|m| match m {
        JobMessage::Custom(_, payload) => payload.into_any().downcast::<T>().ok(),
        _ => None,
    })
}
