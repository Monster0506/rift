use super::super::Editor;
use crate::error::RiftError;
use crate::notification::NotificationType;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn notify(&mut self, kind: NotificationType, message: impl Into<String>) {
        self.state.notifications.notify(kind, message);
    }

    pub(crate) fn notify_error(&mut self, err: RiftError) {
        self.notify(err.severity.into(), err.message);
    }
}

#[cfg(test)]
#[path = "notify_tests.rs"]
mod notify_tests;
