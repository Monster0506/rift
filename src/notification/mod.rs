//! Notification system
//! Manages popup notifications for the user

use crate::error::ErrorSeverity;
use std::time::{Duration, Instant};

/// Types of notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
    /// Informational message
    Info,
    /// Warning message
    Warning,
    /// Error message
    Error,
    /// Success message
    Success,
}

impl From<ErrorSeverity> for NotificationType {
    fn from(severity: ErrorSeverity) -> Self {
        match severity {
            ErrorSeverity::Info => NotificationType::Info,
            ErrorSeverity::Warning => NotificationType::Warning,
            ErrorSeverity::Error => NotificationType::Error,
            ErrorSeverity::Critical => NotificationType::Error,
        }
    }
}

/// A single notification
#[derive(Debug, Clone)]
pub struct Notification {
    /// Unique identifier
    pub id: u64,
    /// The message content
    pub message: String,
    /// The type/severity of the notification
    pub kind: NotificationType,
    /// When the notification was created
    pub timestamp: Instant,
    /// Optional time-to-live. If None, it persists until manually dismissed.
    pub ttl: Option<Duration>,
}

impl Notification {
    /// Create a new notification
    pub fn new(
        id: u64,
        kind: NotificationType,
        message: impl Into<String>,
        ttl: Option<Duration>,
    ) -> Self {
        Self {
            id,
            message: message.into(),
            kind,
            timestamp: Instant::now(),
            ttl,
        }
    }

    /// Check if the notification has expired
    pub fn is_expired(&self, now: Instant) -> bool {
        if let Some(ttl) = self.ttl {
            now.duration_since(self.timestamp) > ttl
        } else {
            false
        }
    }
}

/// Manages active notifications
pub struct NotificationManager {
    /// Active notifications
    notifications: Vec<Notification>,
    /// Counter for generating unique IDs
    next_id: u64,
    /// Monotonic generation counter for change detection
    pub generation: u64,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            next_id: 0,
            generation: 0,
        }
    }

    /// Add a notification
    pub fn add(
        &mut self,
        kind: NotificationType,
        message: impl Into<String>,
        ttl: Option<Duration>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.notifications
            .push(Notification::new(id, kind, message, ttl));
        self.generation += 1;
        id
    }

    /// Add an info notification (convenience)
    pub fn info(&mut self, message: impl Into<String>) -> u64 {
        self.add(
            NotificationType::Info,
            message,
            Some(Duration::from_secs(5)),
        )
    }

    /// Add a warning notification (convenience)
    pub fn warn(&mut self, message: impl Into<String>) -> u64 {
        self.add(
            NotificationType::Warning,
            message,
            Some(Duration::from_secs(8)),
        )
    }

    /// Add an error notification (convenience)
    pub fn error(&mut self, message: impl Into<String>) -> u64 {
        // Errors default to no TTL (must be dismissed?) or longer TTL?
        // Let's stick to longer TTL for now as per "popup notifications" usually disappear.
        // User feedback can adjust this.
        self.add(
            NotificationType::Error,
            message,
            Some(Duration::from_secs(10)),
        )
    }

    /// Add a success notification (convenience)
    pub fn success(&mut self, message: impl Into<String>) -> u64 {
        self.add(
            NotificationType::Success,
            message,
            Some(Duration::from_secs(3)),
        )
    }

    /// Check if there are any notifications
    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }

    /// Get active (non-expired) notifications
    /// This also lazily prunes expired notifications?
    /// No, let's have explicit prune. And `iter_active` just returns iterator.
    pub fn iter_active(&self) -> std::slice::Iter<'_, Notification> {
        self.notifications.iter()
    }

    /// Prune expired notifications
    pub fn prune_expired(&mut self) {
        let now = Instant::now();
        let old_len = self.notifications.len();
        self.notifications.retain(|n| !n.is_expired(now));
        if self.notifications.len() != old_len {
            self.generation += 1;
        }
    }

    /// Remove a notification by ID
    pub fn remove(&mut self, id: u64) {
        if let Some(pos) = self.notifications.iter().position(|n| n.id == id) {
            self.notifications.remove(pos);
            self.generation += 1;
        }
    }

    /// Clear the last notification
    pub fn clear_last(&mut self) {
        if self.notifications.pop().is_some() {
            self.generation += 1;
        }
    }

    /// Clear all notifications
    pub fn clear_all(&mut self) {
        if !self.notifications.is_empty() {
            self.notifications.clear();
            self.generation += 1;
        }
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
