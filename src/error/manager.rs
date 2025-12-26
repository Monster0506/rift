//! Error Manager
//! Centralizes error handling and notification dispatch

use crate::error::RiftError;
use crate::notification::NotificationManager;

/// Manages errors and their presentation to the user
pub struct ErrorManager {
    /// Internal notification manager for displaying errors
    notifications: NotificationManager,
}

impl ErrorManager {
    /// Create a new error manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            notifications: NotificationManager::new(),
        }
    }

    pub fn handle(&mut self, err: RiftError) {
        let message = err.message.clone();
        match err.severity {
            crate::error::ErrorSeverity::Critical | crate::error::ErrorSeverity::Error => {
                self.notifications.error(message);
            }
            crate::error::ErrorSeverity::Warning => {
                self.notifications.warn(message);
            }
            crate::error::ErrorSeverity::Info => {
                self.notifications.info(message);
            }
        }
    }

    /// Get a reference to the notification manager
    #[must_use]
    pub fn notifications(&self) -> &NotificationManager {
        &self.notifications
    }

    /// Get a mutable reference to the notification manager
    pub fn notifications_mut(&mut self) -> &mut NotificationManager {
        &mut self.notifications
    }
}

impl Default for ErrorManager {
    fn default() -> Self {
        Self::new()
    }
}
