#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Integer(usize),
    Float(f64),
    Enum(String),
    Color(crate::color::Color),
    Path(String),
}

impl SettingValue {
    #[must_use]
    pub fn to_display_string(&self) -> String {
        use crate::color::Color;
        match self {
            SettingValue::Bool(b) => b.to_string(),
            SettingValue::Integer(n) => n.to_string(),
            SettingValue::Float(f) => f.to_string(),
            SettingValue::Enum(s) | SettingValue::Path(s) => s.clone(),
            SettingValue::Color(Color::Reset) => "none".to_string(),
            SettingValue::Color(Color::Black) => "black".to_string(),
            SettingValue::Color(Color::DarkGrey) => "darkgrey".to_string(),
            SettingValue::Color(Color::Red) => "red".to_string(),
            SettingValue::Color(Color::DarkRed) => "darkred".to_string(),
            SettingValue::Color(Color::Green) => "green".to_string(),
            SettingValue::Color(Color::DarkGreen) => "darkgreen".to_string(),
            SettingValue::Color(Color::Yellow) => "yellow".to_string(),
            SettingValue::Color(Color::DarkYellow) => "darkyellow".to_string(),
            SettingValue::Color(Color::Blue) => "blue".to_string(),
            SettingValue::Color(Color::DarkBlue) => "darkblue".to_string(),
            SettingValue::Color(Color::Magenta) => "magenta".to_string(),
            SettingValue::Color(Color::DarkMagenta) => "darkmagenta".to_string(),
            SettingValue::Color(Color::Cyan) => "cyan".to_string(),
            SettingValue::Color(Color::DarkCyan) => "darkcyan".to_string(),
            SettingValue::Color(Color::White) => "white".to_string(),
            SettingValue::Color(Color::Grey) => "grey".to_string(),
            SettingValue::Color(Color::Ansi256(n)) => format!("ansi256({n})"),
            SettingValue::Color(Color::Rgb { r, g, b }) => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SettingType {
    Boolean,
    Integer {
        min: Option<usize>,
        max: Option<usize>,
    },
    Float {
        min: Option<f64>,
        max: Option<f64>,
    },
    Enum {
        variants: &'static [&'static str],
    },
    IntegerOrKeyword {
        min: Option<usize>,
        max: Option<usize>,
        keywords: &'static [&'static str],
    },
    Color,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingError {
    ParseError(String),
    ValidationError(String),
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

pub type SettingSetter<T> = fn(&mut T, SettingValue) -> Result<(), SettingError>;

pub type SettingGetter<T> = fn(&T) -> SettingValue;

#[derive(Debug, Clone)]
pub struct SettingDescriptor<T> {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub ty: SettingType,
    pub set: SettingSetter<T>,
    pub get: Option<SettingGetter<T>>,
    pub needs_full_redraw: bool,
}
