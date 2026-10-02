use crate::color::Color;
use crate::command_line::settings::descriptor::{
    SettingDescriptor, SettingError, SettingType, SettingValue,
};
use crate::command_line::settings::registry::SettingsRegistry;
use crate::floating_window::BorderChars;
use crate::state::UserSettings;

fn create_unicode_border() -> BorderChars {
    BorderChars {
        top_left: '╭',
        top_right: '╮',
        bottom_left: '╰',
        bottom_right: '╯',
        horizontal: '─',
        vertical: '│',
    }
}

fn create_ascii_border() -> BorderChars {
    BorderChars {
        top_left: '+',
        top_right: '+',
        bottom_left: '+',
        bottom_right: '+',
        horizontal: '-',
        vertical: '|',
    }
}

fn set_border_style(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Enum(style) => {
            settings.default_border_chars = match style.as_str() {
                "unicode" => Some(create_unicode_border()),
                "ascii" => Some(create_ascii_border()),
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
fn set_clipboard_size(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Integer(n) => {
            if n < 1 {
                return Err(SettingError::ValidationError(
                    "clipboard.size must be at least 1".to_string(),
                ));
            }
            settings.clipboard_ring_size = n;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected integer".to_string(),
        )),
    }
}

fn get_clipboard_size(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(s.clipboard_ring_size)
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

fn get_cmd_window_width_ratio(s: &UserSettings) -> SettingValue {
    SettingValue::Float(s.command_line_window.width_ratio)
}

fn get_cmd_window_min_width(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(s.command_line_window.min_width)
}

fn get_cmd_window_height(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(s.command_line_window.height)
}

fn get_poll_rate(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(s.poll_timeout_ms as usize)
}

fn get_editor_bg(s: &UserSettings) -> SettingValue {
    SettingValue::Color(s.editor_bg.unwrap_or(Color::Reset))
}

fn get_editor_fg(s: &UserSettings) -> SettingValue {
    SettingValue::Color(s.editor_fg.unwrap_or(Color::Reset))
}

fn set_cursor_color(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Color(color) => {
            settings.cursor_color = if color == Color::Reset {
                None
            } else {
                Some(color)
            };
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected color".to_string())),
    }
}

fn get_cursor_color(s: &UserSettings) -> SettingValue {
    SettingValue::Color(s.cursor_color.unwrap_or(Color::Reset))
}

fn set_cursor_speed(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Float(f) => {
            settings.cursor_speed = f;
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected float".to_string())),
    }
}

fn get_cursor_speed(s: &UserSettings) -> SettingValue {
    SettingValue::Float(s.cursor_speed)
}

fn set_equalize_proportional(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.equalize_proportional = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_lsp_debug_log(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.lsp_debug_log = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_lsp_virtual_text(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.lsp_virtual_text = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_lsp_diagnostic_tooltip(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.lsp_diagnostic_tooltip = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_ghost_cut(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.ghost_cut = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_persistent_undo(
    settings: &mut UserSettings,
    value: SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.persistent_undo = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_undo_dir(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Path(s) => {
            let s = s.trim();
            settings.undo_dir = if s.is_empty() {
                None
            } else {
                Some(crate::history::persist::expand_tilde(s))
            };
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected path".to_string())),
    }
}

fn get_undo_dir(s: &UserSettings) -> SettingValue {
    SettingValue::Path(
        s.undo_dir
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    )
}

pub const SETTINGS: &[SettingDescriptor<UserSettings>] = &[
    SettingDescriptor {
        name: "command_line.borderstyle",
        aliases: &["clborderstyle"],
        description: "Style of the command line window border",
        ty: SettingType::Enum {
            variants: &["unicode", "ascii"],
        },
        set: set_border_style,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.width_ratio",
        aliases: &[],
        description: "Width of command line window as ratio of screen width",
        ty: SettingType::Float {
            min: Some(0.0),
            max: Some(1.0),
        },
        set: set_cmd_window_width_ratio,
        get: Some(get_cmd_window_width_ratio),
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.min_width",
        aliases: &[],
        description: "Minimum width of command line window in columns",
        ty: SettingType::Integer {
            min: Some(1),
            max: None,
        },
        set: set_cmd_window_min_width,
        get: Some(get_cmd_window_min_width),
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.height",
        aliases: &[],
        description: "Height of command line window in rows",
        ty: SettingType::Integer {
            min: Some(1),
            max: None,
        },
        set: set_cmd_window_height,
        get: Some(get_cmd_window_height),
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.border",
        aliases: &[],
        description: "Show border around command line window",
        ty: SettingType::Boolean,
        set: set_cmd_window_border,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.reverse_video",
        aliases: &["clreverse"],
        description: "Use reverse video for command line window",
        ty: SettingType::Boolean,
        set: set_cmd_window_reverse_video,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "appearance.background",
        aliases: &["bg"],
        description: "Editor background color",
        ty: SettingType::Color,
        set: set_editor_bg,
        get: Some(get_editor_bg),
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "appearance.foreground",
        aliases: &["fg"],
        description: "Editor foreground color",
        ty: SettingType::Color,
        set: set_editor_fg,
        get: Some(get_editor_fg),
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "appearance.cursor_color",
        aliases: &["cursorcolor"],
        description: "Cursor block color in Normal mode",
        ty: SettingType::Color,
        set: set_cursor_color,
        get: Some(get_cursor_color),
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "appearance.cursor_speed",
        aliases: &["cursorspeed"],
        description: "Cursor animation speed: fraction of remaining distance covered per frame (0.0-1.0)",
        ty: SettingType::Float {
            min: Some(0.0),
            max: Some(1.0),
        },
        set: set_cursor_speed,
        get: Some(get_cursor_speed),
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "appearance.theme",
        aliases: &["colorscheme"],
        description: "Color theme",
        ty: SettingType::Enum {
            variants: &[
                crate::constants::themes::LIGHT,
                crate::constants::themes::DARK,
                crate::constants::themes::GRUVBOX,
                crate::constants::themes::NORDIC,
            ],
        },
        set: set_theme,
        get: None,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "status_line.show_filename",
        aliases: &[],
        description: "Show filename in status line",
        ty: SettingType::Boolean,
        set: set_show_filename,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.reverse_video",
        aliases: &[],
        description: "Use reverse video for status line",
        ty: SettingType::Boolean,
        set: set_status_line_reverse_video,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.show_status_line",
        aliases: &[],
        description: "Show status line",
        ty: SettingType::Boolean,
        set: set_show_status_line,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.show_dirty",
        aliases: &[],
        description: "Show dirty indicator in status line",
        ty: SettingType::Boolean,
        set: set_show_dirty_indicator,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "number",
        aliases: &[],
        description: "Show line numbers",
        ty: SettingType::Boolean,
        set: set_show_line_numbers,
        get: None,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "clipboard.size",
        aliases: &[],
        description: "Maximum number of entries in the clipboard ring",
        ty: SettingType::Integer {
            min: Some(1),
            max: Some(1000),
        },
        set: set_clipboard_size,
        get: Some(get_clipboard_size),
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "editor.poll_rate",
        aliases: &[],
        description: "Set the polling rate (ms)",
        ty: SettingType::Integer {
            min: Some(1),
            max: Some(10000),
        },
        set: set_poll_rate,
        get: Some(get_poll_rate),
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "split.equalize_proportional",
        aliases: &["eqprop"],
        description: "When on, ^w= distributes space proportionally to leaf count; when off, each split gets 50/50",
        ty: SettingType::Boolean,
        set: set_equalize_proportional,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "lsp.debug_log",
        aliases: &["lspdebug"],
        description: "Show LSP protocol debug messages as notifications",
        ty: SettingType::Boolean,
        set: set_lsp_debug_log,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "lsp.virtual_text",
        aliases: &["lspvt"],
        description: "Render LSP diagnostics as trailing end-of-line virtual text",
        ty: SettingType::Boolean,
        set: set_lsp_virtual_text,
        get: None,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "lsp.diagnostic_tooltip",
        aliases: &["lsptip"],
        description: "Show the diagnostic tooltip for the cursor line",
        ty: SettingType::Boolean,
        set: set_lsp_diagnostic_tooltip,
        get: None,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "ghostcut",
        aliases: &["gc"],
        description: "Delete d-cuts immediately but grey them out in place until something resolves the paint, instead of removing them from the screen right away",
        ty: SettingType::Boolean,
        set: set_ghost_cut,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "undofile",
        aliases: &["udf"],
        description: "Persist undo history to disk so it survives editor restarts",
        ty: SettingType::Boolean,
        set: set_persistent_undo,
        get: None,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "undodir",
        aliases: &["udir"],
        description: "Directory for persisted undo files (default: <config_dir>/undofiles)",
        ty: SettingType::Path,
        set: set_undo_dir,
        get: Some(get_undo_dir),
        needs_full_redraw: false,
    },
];

#[must_use]
pub fn create_settings_registry() -> SettingsRegistry<UserSettings> {
    SettingsRegistry::new(SETTINGS)
}
