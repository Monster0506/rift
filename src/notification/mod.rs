//! Notification system
//! Manages popup notifications for the user

use crate::error::ErrorSeverity;
use crate::time::{Instant, SystemTime};
use std::collections::VecDeque;
use std::time::Duration;

/// Maximum number of entries retained in the persistent message log.
/// Oldest entries are dropped once this cap is exceeded.
const MESSAGE_LOG_CAPACITY: usize = 500;

/// Types of notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationType {
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
            ErrorSeverity::Warning => Self::Warning,
            ErrorSeverity::Error => Self::Error,
            #[cfg(test)]
            ErrorSeverity::Info => Self::Info,
            #[cfg(test)]
            ErrorSeverity::Critical => Self::Error,
        }
    }
}

/// The kind of a job lifecycle event recorded in the message log
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobEventKind {
    Started,
    Progress(u32),
    Finished,
    Error,
    Cancelled,
}

/// A single entry in the persistent message log
#[derive(Debug, Clone)]
pub(crate) enum MessageEntry {
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
    #[must_use]
    pub(crate) const fn time(&self) -> SystemTime {
        match self {
            Self::Notification { time, .. } | Self::JobEvent { time, .. } => *time,
        }
    }
}

/// A single notification
#[derive(Debug, Clone)]
pub(crate) struct Notification {
    /// The message content
    pub(crate) message: String,
    /// The type/severity of the notification
    pub(crate) kind: NotificationType,
    /// When the notification was created
    timestamp: Instant,
    /// Optional time-to-live. If None, it persists until manually dismissed.
    pub(crate) ttl: Option<Duration>,
}

impl Notification {
    /// Create a new notification
    fn new(kind: NotificationType, message: impl Into<String>, ttl: Option<Duration>) -> Self {
        Self {
            message: message.into(),
            kind,
            timestamp: Instant::now(),
            ttl,
        }
    }

    /// Check if the notification has expired
    #[must_use]
    fn is_expired(&self, now: Instant) -> bool {
        self.ttl
            .is_some_and(|ttl| now.duration_since(self.timestamp) > ttl)
    }
}

/// Manages active notifications
pub(crate) struct NotificationManager {
    /// Active notifications
    notifications: Vec<Notification>,
    /// Monotonic generation counter for change detection
    pub(crate) generation: u64,
    /// Ring buffer log of recent messages and job events, capped at
    /// `MESSAGE_LOG_CAPACITY` entries.
    message_log: VecDeque<MessageEntry>,
    last_render_time: Option<Instant>,
}

impl NotificationManager {
    /// Create a new notification manager
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            notifications: Vec::new(),
            generation: 0,
            message_log: VecDeque::new(),
            last_render_time: None,
        }
    }

    /// Add a notification
    pub(crate) fn add(
        &mut self,
        kind: NotificationType,
        message: impl Into<String>,
        ttl: Option<Duration>,
    ) {
        let message: String = message.into();
        self.push_log_entry(MessageEntry::Notification {
            time: SystemTime::now(),
            kind,
            message: message.clone(),
        });
        self.notifications
            .push(Notification::new(kind, message, ttl));
        self.generation += 1;
    }

    pub(crate) fn log_job_event(
        &mut self,
        job_id: usize,
        kind: JobEventKind,
        silent: bool,
        message: impl Into<String>,
    ) {
        let entry = MessageEntry::JobEvent {
            time: SystemTime::now(),
            job_id,
            kind,
            silent,
            message: message.into(),
        };
        // Coalesce repeated progress ticks for the same job into the latest entry
        // instead of appending one per tick.
        if matches!(kind, JobEventKind::Progress(_)) {
            if let Some(last) = self.message_log.back_mut() {
                if matches!(
                    last,
                    MessageEntry::JobEvent { job_id: last_id, kind: JobEventKind::Progress(_), .. }
                        if *last_id == job_id
                ) {
                    *last = entry;
                    self.generation += 1;
                    return;
                }
            }
        }
        self.push_log_entry(entry);
        self.generation += 1;
    }

    /// Push an entry onto the log, dropping the oldest entry if over capacity.
    fn push_log_entry(&mut self, entry: MessageEntry) {
        if self.message_log.len() >= MESSAGE_LOG_CAPACITY {
            self.message_log.pop_front();
        }
        self.message_log.push_back(entry);
    }

    #[must_use]
    pub(crate) fn message_log(&self) -> Vec<MessageEntry> {
        self.message_log.iter().cloned().collect()
    }

    /// Add an info notification (convenience)
    #[cfg(test)]
    pub(crate) fn info(&mut self, message: impl Into<String>) {
        self.add(
            NotificationType::Info,
            message,
            Some(Duration::from_secs(5)),
        );
    }

    /// Add a warning notification (convenience)
    pub(crate) fn warn(&mut self, message: impl Into<String>) {
        self.add(
            NotificationType::Warning,
            message,
            Some(Duration::from_secs(8)),
        );
    }

    /// Add an error notification (convenience)
    pub(crate) fn error(&mut self, message: impl Into<String>) {
        self.add(
            NotificationType::Error,
            message,
            Some(Duration::from_secs(10)),
        );
    }

    #[cfg(test)]
    /// Add a success notification (convenience)
    fn success(&mut self, message: impl Into<String>) {
        self.add(
            NotificationType::Success,
            message,
            Some(Duration::from_secs(3)),
        );
    }

    /// Check if there are any notifications
    #[must_use]
    pub(crate) const fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }

    pub(crate) fn iter_active(&self) -> std::slice::Iter<'_, Notification> {
        self.notifications.iter()
    }

    /// Returns true if the display needs a re-render due to time passing.
    #[must_use]
    pub(crate) fn tick(&self, poll_timeout: Duration) -> bool {
        if self.notifications.is_empty() {
            return false;
        }
        let now = Instant::now();
        let last = self.last_render_time.unwrap_or(now);
        for n in &self.notifications {
            if let Some(ttl) = n.ttl {
                let elapsed_now = now.saturating_duration_since(n.timestamp);
                let remaining = ttl.saturating_sub(elapsed_now);
                if remaining <= poll_timeout {
                    return true;
                }
                let elapsed_last = last.saturating_duration_since(n.timestamp);
                if elapsed_now.as_secs() != elapsed_last.as_secs() {
                    return true;
                }
            }
        }
        false
    }

    /// Record that a render has completed.
    pub(crate) fn mark_rendered(&mut self) {
        self.last_render_time = Some(Instant::now());
    }

    /// Prune expired notifications
    pub(crate) fn prune_expired(&mut self) {
        let now = Instant::now();
        let old_len = self.notifications.len();
        self.notifications.retain(|n| !n.is_expired(now));
        if self.notifications.len() != old_len {
            self.generation += 1;
        }
    }

    /// Clear the last notification
    pub(crate) fn clear_last(&mut self) {
        if self.notifications.pop().is_some() {
            self.generation += 1;
        }
    }

    /// Clear all notifications
    pub(crate) fn clear_all(&mut self) {
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
