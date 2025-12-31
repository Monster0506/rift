//! Settings definitions
//! Declarative registry of all :set command options

use crate::command_line::settings::descriptor::{
    SettingDescriptor, SettingError, SettingType, SettingValue,
};
use crate::command_line::settings::registry::SettingsRegistry;
use crate::floating_window::BorderChars;
use crate::state::UserSettings;

// Helper functions to create border presets
fn create_unicode_border() -> BorderChars {
    BorderChars {
        top_left: vec![0xE2, 0x95, 0xAD],     // ╭
        top_right: vec![0xE2, 0x95, 0xAE],    // ╮
        bottom_left: vec![0xE2, 0x95, 0xB0],  // ╰
        bottom_right: vec![0xE2, 0x95, 0xAF], // ╯
        horizontal: vec![0xE2, 0x94, 0x80],   // ─
        vertical: vec![0xE2, 0x94, 0x82],     // │
    }
}

fn create_ascii_border() -> BorderChars {
    BorderChars {
        top_left: vec![b'+'],
        top_right: vec![b'+'],
        bottom_left: vec![b'+'],
        bottom_right: vec![b'+'],
        horizontal: vec![b'-'],
        vertical: vec![b'|'],
    }
}

// Setter functions for each setting

fn set_border_style(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Enum(style) => {
            settings.default_border_chars = match style.as_str() {
                "unicode" => Some(create_unicode_border()),
                "ascii" => Some(create_ascii_border()),
                "none" => None,
                _ => {
                    return Err(SettingError::ValidationError(format!(
                        "Unknown border style: {style}"
                    )))
                }
            };
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected enum".to_string())),
    }
}

fn set_cmd_window_width_ratio(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Float(f) => {
            settings.command_line_window.width_ratio = f;
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected float".to_string())),
    }
}

fn set_cmd_window_min_width(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Integer(n) => {
            settings.command_line_window.min_width = n;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected integer".to_string(),
        )),
    }
}
fn set_poll_rate(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Integer(n) => {
            settings.poll_timeout_ms = n as u64;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected integer".to_string(),
        )),
    }
}

fn set_cmd_window_height(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Integer(n) => {
            if n == 0 {
                return Err(SettingError::ValidationError(
                    "height must be greater than 0".to_string(),
                ));
            }
            settings.command_line_window.height = n;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected integer".to_string(),
        )),
    }
}

fn set_cmd_window_border(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.command_line_window.border = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_cmd_window_reverse_video(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.command_line_window.reverse_video = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_editor_bg(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Color(color) => {
            settings.editor_bg = if color == crate::color::Color::Reset {
                None
            } else {
                Some(color)
            };
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected color".to_string())),
    }
}

fn set_editor_fg(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Color(color) => {
            settings.editor_fg = if color == crate::color::Color::Reset {
                None
            } else {
                Some(color)
            };
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected color".to_string())),
    }
}

fn set_theme(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Enum(theme_name) => {
            if let Some(theme) = crate::color::Theme::by_name(&theme_name) {
                // Apply theme using the theme handler
                // This allows themes to apply more than just background/foreground
                theme.apply_to_settings(settings);
                Ok(())
            } else {
                Err(SettingError::ValidationError(format!(
                    "Unknown theme: {}. Available themes: {}",
                    theme_name,
                    crate::color::Theme::available_themes().join(", ")
                )))
            }
        }
        _ => Err(SettingError::ValidationError(
            "Expected theme name".to_string(),
        )),
    }
}

fn set_show_filename(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.status_line.show_filename = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_show_dirty_indicator(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.status_line.show_dirty_indicator = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_show_line_numbers(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.show_line_numbers = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_status_line_reverse_video(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.status_line.reverse_video = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_show_status_line(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.status_line.show_status_line = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

/// Static registry of all settings
pub const SETTINGS: &[SettingDescriptor<UserSettings>] = &[
    SettingDescriptor {
        name: "command_line.borderstyle",
        aliases: &["clborderstyle"],
        description: "Style of the command line window border",
        ty: SettingType::Enum {
            variants: &["unicode", "ascii", "none"],
        },
        set: set_border_style,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.width_ratio",
        aliases: &["clwidthratio"],
        description: "Width of command line window as ratio of screen width",
        ty: SettingType::Float {
            min: Some(0.0),
            max: Some(1.0),
        },
        set: set_cmd_window_width_ratio,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.min_width",
        aliases: &["clminwidth"],
        description: "Minimum width of command line window in columns",
        ty: SettingType::Integer {
            min: Some(1),
            max: None,
        },
        set: set_cmd_window_min_width,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.height",
        aliases: &["clheight"],
        description: "Height of command line window in rows",
        ty: SettingType::Integer {
            min: Some(1),
            max: None,
        },
        set: set_cmd_window_height,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.border",
        aliases: &["clborder"],
        description: "Show border around command line window",
        ty: SettingType::Boolean,
        set: set_cmd_window_border,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.reverse_video",
        aliases: &["clreverse"],
        description: "Use reverse video for command line window",
        ty: SettingType::Boolean,
        set: set_cmd_window_reverse_video,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "appearance.background",
        aliases: &["apbg", "bg"],
        description: "Editor background color",
        ty: SettingType::Color,
        set: set_editor_bg,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "appearance.foreground",
        aliases: &["apfg", "fg"],
        description: "Editor foreground color",
        ty: SettingType::Color,
        set: set_editor_fg,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "appearance.theme",
        aliases: &["aptheme", "colorscheme", "colors"],
        description: "Color theme",
        ty: SettingType::Enum {
            variants: &["light", "dark", "gruvbox", "nordic", "nord"],
        },
        set: set_theme,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "status_line.show_filename",
        aliases: &["slfilename"],
        description: "Show filename in status line",
        ty: SettingType::Boolean,
        set: set_show_filename,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.reverse_video",
        aliases: &["slreverse"],
        description: "Use reverse video for status line",
        ty: SettingType::Boolean,
        set: set_status_line_reverse_video,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.show_status_line",
        aliases: &["slshow"],
        description: "Show status line",
        ty: SettingType::Boolean,
        set: set_show_status_line,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.show_dirty",
        aliases: &["sldirty"],
        description: "Show dirty indicator in status line",
        ty: SettingType::Boolean,
        set: set_show_dirty_indicator,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "number",
        aliases: &[],
        description: "Show line numbers",
        ty: SettingType::Boolean,
        set: set_show_line_numbers,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "editor.poll_rate",
        aliases: &[],
        description: "Set the polling rate (ms)",
        ty: SettingType::Integer {
            min: Some(1),
            max: Some(1000),
        },
        set: set_poll_rate,
        needs_full_redraw: false,
    },
];

/// Create the settings registry
#[must_use]
pub fn create_settings_registry() -> SettingsRegistry<UserSettings> {
    SettingsRegistry::new(SETTINGS)
}
