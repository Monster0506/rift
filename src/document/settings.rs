use crate::command_line::settings::{
    SettingDescriptor, SettingError, SettingType, SettingValue, SettingsRegistry,
};
use crate::document::LineEnding;

/// Document-specific options
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentOptions {
    /// Line ending character(s) to use for this document
    pub line_ending: LineEnding,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        DocumentOptions {
            line_ending: LineEnding::LF,
        }
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
const DOCUMENT_SETTINGS: &[SettingDescriptor<DocumentOptions>] = &[SettingDescriptor {
    name: "line_ending",
    aliases: &["ff", "fileformat"], // mimicking vim's fileformat
    ty: SettingType::Enum {
        variants: &["lf", "crlf", "unix", "dos", "windows"],
    },
    set: set_line_ending,
}];

pub fn create_document_settings_registry() -> SettingsRegistry<DocumentOptions> {
    SettingsRegistry::new(DOCUMENT_SETTINGS)
}
