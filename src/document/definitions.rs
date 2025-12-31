use crate::command_line::settings::{
    SettingDescriptor, SettingError, SettingType, SettingValue, SettingsRegistry,
};
use crate::document::LineEnding;

/// Document-specific options
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentOptions {
    /// Line ending character(s) to use for this document
    pub line_ending: LineEnding,

    /// Tab width in spaces (for display and expansion)
    pub tab_width: usize,
    /// Whether to expand tabs to spaces when inserting
    pub expand_tabs: bool,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        DocumentOptions {
            line_ending: LineEnding::LF,
            tab_width: 4,      // Default tab width
            expand_tabs: true, // Default to expanding tabs to spaces
        }
    }
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

/// Document-specific settings
/// LOCAL_SETTINGS
const DOCUMENT_SETTINGS: &[SettingDescriptor<DocumentOptions>] = &[
    SettingDescriptor {
        name: "line_ending",
        aliases: &["ff", "fileformat"], // mimicking vim's fileformat
        description: "Line ending format (lf/crlf)",
        ty: SettingType::Enum {
            variants: &["lf", "crlf", "unix", "dos", "windows"],
        },
        set: set_line_ending,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "expandtabs",
        aliases: &["et"],
        description: "Use spaces instead of tabs",
        ty: SettingType::Boolean,
        set: set_expand_tabs,
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
        needs_full_redraw: true,
    },
];

pub fn create_document_settings_registry() -> SettingsRegistry<DocumentOptions> {
    SettingsRegistry::new(DOCUMENT_SETTINGS)
}
