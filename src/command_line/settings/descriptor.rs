//! Setting descriptor types
//! Type definitions for declarative setting configuration

/// Typed value after parsing and validation
/// Setters receive this, never raw strings
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    /// Boolean value
    Bool(bool),
    /// Integer value
    Integer(usize),
    /// Floating point value
    Float(f64),
    /// Enum value (canonicalized identifier)
    Enum(String),
    /// Color value
    Color(crate::color::Color),
}

/// Setting type definition for parsing and validation
#[derive(Debug, Clone)]
pub enum SettingType {
    /// Boolean setting (true/false, on/off, yes/no, 1/0)
    Boolean,
    /// Integer setting with optional min/max bounds
    Integer {
        /// Minimum value (inclusive)
        min: Option<usize>,
        /// Maximum value (inclusive)
        max: Option<usize>,
    },
    /// Float setting with optional min/max bounds
    Float {
        /// Minimum value (inclusive)
        min: Option<f64>,
        /// Maximum value (inclusive)
        max: Option<f64>,
    },
    /// Enum setting with static variant list
    Enum {
        /// Valid enum variants (static string slices)
        variants: &'static [&'static str],
    },
    /// Color setting (supports color names, RGB, and 256-color indices)
    Color,
}

/// Structured error for setting operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingError {
    /// Failed to parse string value
    ParseError(String),
    /// Value failed validation (out of range, etc.)
    ValidationError(String),
    /// Unknown option name
    UnknownOption(String),
}

impl std::fmt::Display for SettingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingError::ParseError(msg) => write!(f, "Parse error: {msg}"),
            SettingError::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            SettingError::UnknownOption(name) => write!(f, "Unknown option: {name}"),
        }
    }
}

impl From<SettingError> for crate::error::RiftError {
    fn from(err: SettingError) -> Self {
        use crate::error::{ErrorSeverity, ErrorType, RiftError};
        match err {
            SettingError::ParseError(msg) => RiftError {
                severity: ErrorSeverity::Error,
                kind: ErrorType::Parse,
                code: "SETTING_PARSE_ERROR".to_string(),
                message: msg,
            },
            SettingError::ValidationError(msg) => RiftError {
                severity: ErrorSeverity::Error,
                kind: ErrorType::Settings,
                code: "SETTING_VALIDATION_ERROR".to_string(),
                message: msg,
            },
            SettingError::UnknownOption(name) => RiftError {
                severity: ErrorSeverity::Error,
                kind: ErrorType::Settings,
                code: "UNKNOWN_SETTING".to_string(),
                message: format!("Unknown option: {name}"),
            },
        }
    }
}

/// Setter function signature
///
/// Function pointers (not trait objects) for static dispatch.
/// Receives parsed and validated `SettingValue`, never raw strings.
pub type SettingSetter<T> = fn(&mut T, SettingValue) -> Result<(), SettingError>;

/// Setting descriptor
///
/// Minimal configuration: name, aliases, type, and setter function.
/// Name encodes path (e.g., "`command_line_window.width_ratio`" for nested settings).
/// Setter handles mutation - no separate path information needed.
#[derive(Debug, Clone)]
pub struct SettingDescriptor<T> {
    /// Canonical setting name (e.g., "expandtabs" or "`command_line_window.width_ratio`")
    pub name: &'static str,
    /// Short aliases (e.g., &["et"])
    pub aliases: &'static [&'static str],
    /// Setting type for parsing and validation
    pub ty: SettingType,
    /// Setter function pointer
    pub set: SettingSetter<T>,
    /// Whether setting this option requires a full screen redraw
    pub needs_full_redraw: bool,
}
