//! Notification system
//! Manages popup notifications for the user

use crate::error::ErrorSeverity;
use std::time::{Duration, Instant, SystemTime};

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

/// The kind of a job lifecycle event recorded in the message log
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobEventKind {
    Started,
    Progress(u32),
    Finished,
    Error,
    Cancelled,
}

/// A single entry in the persistent message log
#[derive(Debug, Clone)]
pub enum MessageEntry {
    /// A user-facing notification
    Notification {
        time: SystemTime,
        kind: NotificationType,
        message: String,
    },
    /// A job lifecycle event
    JobEvent {
        time: SystemTime,
        job_id: usize,
        kind: JobEventKind,
        /// Whether the job that emitted this is silent
        silent: bool,
        message: String,
    },
}

impl MessageEntry {
    pub fn time(&self) -> SystemTime {
        match self {
            MessageEntry::Notification { time, .. } => *time,
            MessageEntry::JobEvent { time, .. } => *time,
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
    /// Persistent log of all messages and job events (never pruned)
    message_log: Vec<MessageEntry>,
    last_render_time: Option<Instant>,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            next_id: 0,
            generation: 0,
            message_log: Vec::new(),
            last_render_time: None,
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
        let message: String = message.into();
        self.message_log.push(MessageEntry::Notification {
            time: SystemTime::now(),
            kind,
            message: message.clone(),
        });
        self.notifications
            .push(Notification::new(id, kind, message, ttl));
        self.generation += 1;
        id
    }

    pub fn log_job_event(
        &mut self,
        job_id: usize,
        kind: JobEventKind,
        silent: bool,
        message: impl Into<String>,
    ) {
        self.message_log.push(MessageEntry::JobEvent {
            time: SystemTime::now(),
            job_id,
            kind,
            silent,
            message: message.into(),
        });
        self.generation += 1;
    }

    pub fn message_log(&self) -> &[MessageEntry] {
        &self.message_log
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

    pub fn iter_active(&self) -> std::slice::Iter<'_, Notification> {
        self.notifications.iter()
    }

    /// Returns true if the display needs a re-render due to time passing.
    pub fn tick(&self, poll_timeout: Duration) -> bool {
        if self.notifications.is_empty() {
            return false;
        }
        let now = Instant::now();
        let last = self.last_render_time.unwrap_or(now);
        for n in &self.notifications {
            if let Some(ttl) = n.ttl {
                let elapsed_now = now.saturating_duration_since(n.timestamp);
                let remaining = ttl.saturating_sub(elapsed_now);
                // About to expire within the next poll window
                if remaining <= poll_timeout {
                    return true;
                }
                // Crossed a whole-second boundary since last render
                let elapsed_last = last.saturating_duration_since(n.timestamp);
                if elapsed_now.as_secs() != elapsed_last.as_secs() {
                    return true;
                }
            }
        }
        false
    }

    /// Record that a render has completed.
    pub fn mark_rendered(&mut self) {
        self.last_render_time = Some(Instant::now());
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
