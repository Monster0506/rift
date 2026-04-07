use crate::command_line::settings::{
    SettingDescriptor, SettingError, SettingType, SettingValue, SettingsRegistry,
};
use crate::document::LineEnding;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapMode {
    Off,
    Expr(String),
}

impl WrapMode {
    pub fn resolve(&self, terminal_width: usize) -> usize {
        match self {
            WrapMode::Off => 0,
            WrapMode::Expr(s) => crate::eval::eval(s, &|kw| {
                if kw == "auto" {
                    Some(terminal_width)
                } else {
                    None
                }
            })
            .unwrap_or(terminal_width)
            .max(1),
        }
    }
}

/// Document-specific options
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentOptions {
    /// Line ending character(s) to use for this document
    pub line_ending: LineEnding,

    /// Tab width in spaces (for display and expansion)
    pub tab_width: usize,
    /// Whether to expand tabs to spaces when inserting
    pub expand_tabs: bool,
    /// Whether to show line numbers for this document
    pub show_line_numbers: bool,
    /// Per-document wrap setting: None = inherit global, Some(mode) = override
    pub wrap: Option<WrapMode>,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        DocumentOptions {
            line_ending: LineEnding::LF,
            tab_width: 4,
            expand_tabs: true,
            show_line_numbers: true,
            wrap: Some(WrapMode::Expr("auto".to_string())),
        }
    }
}

fn get_tab_width(options: &DocumentOptions) -> String {
    options.tab_width.to_string()
}

fn set_tab_width(options: &mut DocumentOptions, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Integer(n) => {
            if n == 0 {
                return Err(SettingError::ValidationError(
                    "tabwidth must be greater than 0".to_string(),
                ));
            }
            options.tab_width = n;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected integer".to_string(),
        )),
    }
}
fn set_expand_tabs(options: &mut DocumentOptions, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            options.expand_tabs = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_number(options: &mut DocumentOptions, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            options.show_line_numbers = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_line_ending(options: &mut DocumentOptions, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Enum(s) => match s.to_lowercase().as_str() {
            "lf" | "unix" => {
                options.line_ending = LineEnding::LF;
                Ok(())
            }
            "crlf" | "windows" | "dos" => {
                options.line_ending = LineEnding::CRLF;
                Ok(())
            }
            _ => Err(SettingError::ValidationError(format!(
                "Invalid line ending: {}. Expected 'lf' or 'crlf'",
                s
            ))),
        },
        _ => Err(SettingError::ValidationError(
            "Expected enum value for line ending".to_string(),
        )),
    }
}

fn get_wrap(options: &DocumentOptions) -> String {
    match &options.wrap {
        None | Some(WrapMode::Off) => "0".to_string(),
        Some(WrapMode::Expr(s)) => s.clone(),
    }
}

fn set_wrap(options: &mut DocumentOptions, value: SettingValue) -> Result<(), SettingError> {
    let expr = match value {
        SettingValue::Integer(0) => {
            options.wrap = Some(WrapMode::Off);
            return Ok(());
        }
        SettingValue::Integer(n) => n.to_string(),
        SettingValue::Enum(s) => s,
        _ => {
            return Err(SettingError::ValidationError(
                "Expected integer or expression".to_string(),
            ))
        }
    };
    options.wrap = Some(WrapMode::Expr(expr));
    Ok(())
}

/// Document-specific settings
/// LOCAL_SETTINGS
const DOCUMENT_SETTINGS: &[SettingDescriptor<DocumentOptions>] = &[
    SettingDescriptor {
        name: "number",
        aliases: &["nu"],
        description: "Show line numbers",
        ty: SettingType::Boolean,
        set: set_number,
        get: None,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "line_ending",
        aliases: &["ff", "fileformat"], // mimicking vim's fileformat
        description: "Line ending format (lf/crlf)",
        ty: SettingType::Enum {
            variants: &["lf", "crlf", "unix", "dos", "windows"],
        },
        set: set_line_ending,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "expandtabs",
        aliases: &["et"],
        description: "Use spaces instead of tabs",
        ty: SettingType::Boolean,
        set: set_expand_tabs,
        get: None,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "tabwidth",
        aliases: &["tw"],
        description: "Number of spaces per tab",
        ty: SettingType::Integer {
            min: Some(1),
            max: None,
        },
        set: set_tab_width,
        get: Some(get_tab_width),
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "wrap",
        aliases: &[],
        description: "Soft-wrap column (0 = off, n = wrap at column n)",
        ty: SettingType::IntegerOrKeyword {
            min: None,
            max: None,
            keywords: &["auto"],
        },
        set: set_wrap,
        get: Some(get_wrap),
        needs_full_redraw: true,
    },
];

pub fn create_document_settings_registry() -> SettingsRegistry<DocumentOptions> {
    SettingsRegistry::new(DOCUMENT_SETTINGS)
}
