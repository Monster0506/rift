//! Centralized error handling for Rift
//! Defines common error types, severity levels, and error codes

use std::fmt;

/// Severity level of an error
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ErrorSeverity {
    /// Informational message (not really an error)
    #[cfg(test)]
    Info,
    /// Warning - something might be wrong but operation can continue
    Warning,
    /// Standard error - operation failed but editor can continue
    Error,
    /// Critical error - may lead to data loss or require restart
    #[cfg(test)]
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(test)]
            Self::Info => write!(f, "{}", logging::INFO),
            Self::Warning => write!(f, "{}", logging::WARN),
            Self::Error => write!(f, "{}", logging::ERROR),
            #[cfg(test)]
            Self::Critical => write!(f, "{}", logging::CRITICAL),
        }
    }
}

/// Category of the error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorType {
    /// File system or I/O errors
    Io,
    /// Command line parsing errors
    Parse,
    /// Configuration or settings errors
    Settings,
    /// Command execution errors
    Execution,
    /// Rendering or terminal backend errors
    Renderer,
    /// Internal logic or invariant violations
    Internal,
    /// Errors that don't fit other categories
    Other,
}

impl fmt::Display for ErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io => write!(f, "{}", crate::constants::error_types::IO),
            Self::Parse => write!(f, "{}", crate::constants::error_types::PARSE),
            Self::Settings => write!(f, "{}", crate::constants::error_types::SETTINGS),
            Self::Execution => write!(f, "{}", crate::constants::error_types::EXECUTION),
            Self::Renderer => write!(f, "{}", crate::constants::error_types::RENDERER),
            Self::Internal => write!(f, "{}", crate::constants::error_types::INTERNAL),
            Self::Other => write!(f, "{}", crate::constants::error_types::OTHER),
        }
    }
}

/// A structured error in Rift
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiftError {
    /// How serious the error is
    pub(crate) severity: ErrorSeverity,
    /// What kind of error occurred
    pub(crate) kind: ErrorType,
    /// Machine-readable error code (e.g., "E001", "`FILE_NOT_FOUND`")
    pub code: String,
    /// Human-readable description
    pub message: String,
}

impl RiftError {
    /// Create a new standard error (Severity: Error)
    pub(crate) fn new(
        kind: ErrorType,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: ErrorSeverity::Error,
            kind,
            code: code.into(),
            message: message.into(),
        }
    }

    #[cfg(test)]
    /// Create a new critical error (Severity: Critical)
    fn critical(kind: ErrorType, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ErrorSeverity::Critical,
            kind,
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create a new warning (Severity: Warning)
    pub(crate) fn warning(
        kind: ErrorType,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: ErrorSeverity::Warning,
            kind,
            code: code.into(),
            message: message.into(),
        }
    }

    #[cfg(test)]
    /// Check if the message contains a substring (useful for tests)
    #[must_use]
    pub(crate) fn contains_msg(&self, sub: &str) -> bool {
        self.message.contains(sub)
    }
}

impl fmt::Display for RiftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}({}): {}",
            self.severity, self.kind, self.code, self.message
        )
    }
}

impl std::error::Error for RiftError {}

impl From<String> for RiftError {
    fn from(msg: String) -> Self {
        Self::new(ErrorType::Other, "GENERIC_ERROR", msg)
    }
}

impl From<&str> for RiftError {
    fn from(msg: &str) -> Self {
        Self::new(ErrorType::Other, "GENERIC_ERROR", msg)
    }
}

impl From<std::io::Error> for RiftError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorType::Io, "IO_ERROR", err.to_string())
    }
}

#[cfg(test)]
/// Result alias for Rift operations
type Result<T> = std::result::Result<T, RiftError>;

pub(crate) mod manager;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

mod logging {
    #[cfg(test)]
    pub(super) const INFO: &str = "INFO";
    pub(super) const WARN: &str = "WARN";
    pub(super) const ERROR: &str = "ERROR";
    #[cfg(test)]
    pub(super) const CRITICAL: &str = "CRITICAL";
}
