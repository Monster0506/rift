use super::super::Editor;
use crate::error::RiftError;
use crate::notification::NotificationType;
use crate::term::TerminalBackend;

pub(crate) enum NotificationDelivery {
    Toast(NotificationType),
    #[cfg(feature = "plugins")]
    LogOnly(NotificationType),
}

impl From<NotificationType> for NotificationDelivery {
    fn from(kind: NotificationType) -> Self {
        Self::Toast(kind)
    }
}

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn notify(
        &mut self,
        delivery: impl Into<NotificationDelivery>,
        message: impl Into<String>,
    ) {
        match delivery.into() {
            NotificationDelivery::Toast(kind) => self.state.notifications.notify(kind, message),
            #[cfg(feature = "plugins")]
            NotificationDelivery::LogOnly(kind) => self.state.notifications.log(kind, message),
        }
    }

    pub(crate) fn notify_error(&mut self, err: RiftError) {
        self.notify(NotificationType::from(err.severity), err.message);
    }
}

#[cfg(test)]
#[path = "notify_tests.rs"]
mod notify_tests;
