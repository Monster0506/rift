#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SettingValue {
    Bool(bool),
    Integer(usize),
    Float(f64),
    Enum(String),
    Color(crate::color::Color),
    Path(String),
}

impl SettingValue {
    #[must_use]
    pub(crate) fn to_display_string(&self) -> String {
        use crate::color::Color;
        match self {
            Self::Bool(b) => b.to_string(),
            Self::Integer(n) => n.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Enum(s) | Self::Path(s) => s.clone(),
            Self::Color(Color::Reset) => "none".to_string(),
            Self::Color(Color::Black) => "black".to_string(),
            Self::Color(Color::DarkGrey) => "darkgrey".to_string(),
            Self::Color(Color::Red) => "red".to_string(),
            Self::Color(Color::DarkRed) => "darkred".to_string(),
            Self::Color(Color::Green) => "green".to_string(),
            Self::Color(Color::DarkGreen) => "darkgreen".to_string(),
            Self::Color(Color::Yellow) => "yellow".to_string(),
            Self::Color(Color::DarkYellow) => "darkyellow".to_string(),
            Self::Color(Color::Blue) => "blue".to_string(),
            Self::Color(Color::DarkBlue) => "darkblue".to_string(),
            Self::Color(Color::Magenta) => "magenta".to_string(),
            Self::Color(Color::DarkMagenta) => "darkmagenta".to_string(),
            Self::Color(Color::Cyan) => "cyan".to_string(),
            Self::Color(Color::DarkCyan) => "darkcyan".to_string(),
            Self::Color(Color::White) => "white".to_string(),
            Self::Color(Color::Grey) => "grey".to_string(),
            Self::Color(Color::Ansi256(n)) => format!("ansi256({n})"),
            Self::Color(Color::Rgb { r, g, b }) => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SettingType {
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
pub(crate) enum SettingError {
    ParseError(String),
    ValidationError(String),
    #[cfg(test)]
    UnknownOption(String),
}

impl std::fmt::Display for SettingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
            Self::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            #[cfg(test)]
            Self::UnknownOption(name) => write!(f, "Unknown option: {name}"),
        }
    }
}

impl From<SettingError> for crate::error::RiftError {
    fn from(err: SettingError) -> Self {
        use crate::error::{ErrorSeverity, ErrorType};
        match err {
            SettingError::ParseError(msg) => Self {
                severity: ErrorSeverity::Error,
                kind: ErrorType::Parse,
                code: "SETTING_PARSE_ERROR".to_string(),
                message: msg,
            },
            SettingError::ValidationError(msg) => Self {
                severity: ErrorSeverity::Error,
                kind: ErrorType::Settings,
                code: "SETTING_VALIDATION_ERROR".to_string(),
                message: msg,
            },
            #[cfg(test)]
            SettingError::UnknownOption(name) => Self {
                severity: ErrorSeverity::Error,
                kind: ErrorType::Settings,
                code: "UNKNOWN_SETTING".to_string(),
                message: format!("Unknown option: {name}"),
            },
        }
    }
}

pub(crate) type SettingSetter<T> = fn(&mut T, &SettingValue) -> Result<(), SettingError>;

pub(crate) type SettingGetter<T> = fn(&T) -> SettingValue;

#[derive(Debug, Clone)]
pub(crate) struct SettingDescriptor<T> {
    pub(crate) name: &'static str,
    pub(crate) aliases: &'static [&'static str],
    pub(crate) description: &'static str,
    pub(crate) ty: SettingType,
    pub(crate) set: SettingSetter<T>,
    pub(crate) get: Option<SettingGetter<T>>,
    pub(crate) needs_full_redraw: bool,
}
