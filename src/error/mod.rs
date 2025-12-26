//! Centralized error handling for Rift
//! Defines common error types, severity levels, and error codes

use std::fmt;

/// Severity level of an error
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Informational message (not really an error)
    Info,
    /// Warning - something might be wrong but operation can continue
    Warning,
    /// Standard error - operation failed but editor can continue
    Error,
    /// Critical error - may lead to data loss or require restart
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Category of the error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
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
            Self::Io => write!(f, "IO"),
            Self::Parse => write!(f, "Parse"),
            Self::Settings => write!(f, "Settings"),
            Self::Execution => write!(f, "Execution"),
            Self::Renderer => write!(f, "Renderer"),
            Self::Internal => write!(f, "Internal"),
            Self::Other => write!(f, "Other"),
        }
    }
}

/// A structured error in Rift
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiftError {
    /// How serious the error is
    pub severity: ErrorSeverity,
    /// What kind of error occurred
    pub kind: ErrorType,
    /// Machine-readable error code (e.g., "E001", "FILE_NOT_FOUND")
    pub code: String,
    /// Human-readable description
    pub message: String,
}

impl RiftError {
    /// Create a new standard error (Severity: Error)
    pub fn new(kind: ErrorType, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ErrorSeverity::Error,
            kind,
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create a new critical error (Severity: Critical)
    pub fn critical(kind: ErrorType, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ErrorSeverity::Critical,
            kind,
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create a new warning (Severity: Warning)
    pub fn warning(kind: ErrorType, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ErrorSeverity::Warning,
            kind,
            code: code.into(),
            message: message.into(),
        }
    }

    /// Check if the message contains a substring (useful for tests)
    pub fn contains_msg(&self, sub: &str) -> bool {
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

/// Result alias for Rift operations
pub type Result<T> = std::result::Result<T, RiftError>;

/// Helper trait to convert various error types into RiftError
pub trait ToRiftError {
    fn to_rift_error(self) -> RiftError;
}

impl ToRiftError for std::io::Error {
    fn to_rift_error(self) -> RiftError {
        RiftError::new(ErrorType::Io, "IO_ERROR", self.to_string())
    }
}

impl ToRiftError for String {
    fn to_rift_error(self) -> RiftError {
        RiftError::new(ErrorType::Other, "GENERIC_ERROR", self)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
