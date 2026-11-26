use crate::color::Color;
use crate::command_line::settings::descriptor::{
    SettingDescriptor, SettingError, SettingType, SettingValue,
};
use crate::command_line::settings::registry::SettingsRegistry;
use crate::floating_window::BorderChars;
use crate::state::UserSettings;

const fn create_unicode_border() -> BorderChars {
    BorderChars {
        top_left: '╭',
        top_right: '╮',
        bottom_left: '╰',
        bottom_right: '╯',
        horizontal: '─',
        vertical: '│',
    }
}

const fn create_ascii_border() -> BorderChars {
    BorderChars {
        top_left: '+',
        top_right: '+',
        bottom_left: '+',
        bottom_right: '+',
        horizontal: '-',
        vertical: '|',
    }
}

fn set_border_style(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
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
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Float(f) => {
            settings.command_line_window.width_ratio = f;
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected float".to_string())),
    }
}

fn set_cmd_window_min_width(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Integer(n) => {
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
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Integer(n) => {
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

const fn get_clipboard_size(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(s.clipboard_ring_size)
}

fn set_poll_rate(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Integer(n) => {
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
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Integer(n) => {
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
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
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
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.command_line_window.reverse_video = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_editor_bg(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Color(color) => {
            settings.editor_bg = if color == crate::color::Color::Reset {
                None
            } else {
                Some(color)
            };
            settings.refresh_derived_highlights();
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected color".to_string())),
    }
}

fn set_editor_fg(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Color(color) => {
            settings.editor_fg = if color == crate::color::Color::Reset {
                None
            } else {
                Some(color)
            };
            settings.refresh_derived_highlights();
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected color".to_string())),
    }
}

fn set_theme(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Enum(theme_name) => crate::color::theme::Theme::by_name(theme_name)
            .map_or_else(
                || {
                    Err(SettingError::ValidationError(format!(
                        "Unknown theme: {}. Available themes: {}",
                        theme_name,
                        crate::color::theme::Theme::NAMES.join(", ")
                    )))
                },
                |theme| {
                    settings.apply_theme(&theme);
                    Ok(())
                },
            ),
        _ => Err(SettingError::ValidationError(
            "Expected theme name".to_string(),
        )),
    }
}

fn set_show_filename(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.status_line.segments.show_filename = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_show_dirty_indicator(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.status_line.segments.show_dirty_indicator = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_status_line_reverse_video(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
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
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.status_line.show_status_line = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

const fn get_cmd_window_width_ratio(s: &UserSettings) -> SettingValue {
    SettingValue::Float(s.command_line_window.width_ratio)
}

const fn get_cmd_window_min_width(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(s.command_line_window.min_width)
}

const fn get_cmd_window_height(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(s.command_line_window.height)
}

fn get_poll_rate(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(usize::try_from(s.poll_timeout_ms).unwrap_or(usize::MAX))
}

fn get_editor_bg(s: &UserSettings) -> SettingValue {
    SettingValue::Color(s.editor_bg.unwrap_or(Color::Reset))
}

fn get_editor_fg(s: &UserSettings) -> SettingValue {
    SettingValue::Color(s.editor_fg.unwrap_or(Color::Reset))
}

fn get_border_style(s: &UserSettings) -> SettingValue {
    let style = match &s.default_border_chars {
        Some(chars) if *chars == create_ascii_border() => "ascii",
        Some(chars) if *chars != create_unicode_border() => "custom",
        _ => "unicode",
    };
    SettingValue::Enum(style.to_string())
}

const fn get_cmd_window_border(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.command_line_window.border)
}

const fn get_cmd_window_reverse_video(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.command_line_window.reverse_video)
}

fn get_theme(s: &UserSettings) -> SettingValue {
    SettingValue::Enum(s.theme.clone().unwrap_or_default())
}

const fn get_show_filename(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.status_line.segments.show_filename)
}

const fn get_status_line_reverse_video(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.status_line.reverse_video)
}

const fn get_show_status_line(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.status_line.show_status_line)
}

const fn get_show_dirty_indicator(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.status_line.segments.show_dirty_indicator)
}

const fn get_equalize_proportional(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.equalize_proportional)
}

const fn get_lsp_debug_log(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.lsp.debug_log)
}

const fn get_lsp_virtual_text(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.lsp.virtual_text)
}

const fn get_lsp_diagnostic_tooltip(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.lsp.diagnostic_tooltip)
}

const fn get_ghost_cut(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.editing.ghost_cut)
}

const fn get_quit_on_q(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.editing.quit_on_q)
}

const fn get_persistent_undo(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.undo.persistent)
}

fn set_cursor_color(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Color(color) => {
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

fn set_selection_bg(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Color(color) => {
            settings.selection_bg = if color == Color::Reset {
                None
            } else {
                Some(color)
            };
            settings.refresh_derived_highlights();
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected color".to_string())),
    }
}

fn get_selection_bg(s: &UserSettings) -> SettingValue {
    SettingValue::Color(
        selection_style(s)
            .and_then(|style| style.bg)
            .unwrap_or(Color::Reset),
    )
}

fn set_selection_fg(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Color(color) => {
            settings.selection_fg = if color == Color::Reset {
                None
            } else {
                Some(color)
            };
            settings.refresh_derived_highlights();
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected color".to_string())),
    }
}

fn get_selection_fg(s: &UserSettings) -> SettingValue {
    SettingValue::Color(
        selection_style(s)
            .and_then(|style| style.fg)
            .unwrap_or(Color::Reset),
    )
}

fn selection_style(s: &UserSettings) -> Option<crate::layer::CellStyle> {
    s.syntax_colors
        .as_ref()
        .and_then(|colors| colors.exact_style("ui.selection"))
}

fn set_cursor_speed(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Float(f) => {
            settings.cursor_speed = f;
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected float".to_string())),
    }
}

const fn get_cursor_speed(s: &UserSettings) -> SettingValue {
    SettingValue::Float(s.cursor_speed)
}

fn set_equalize_proportional(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.equalize_proportional = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_lsp_debug_log(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.lsp.debug_log = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_lsp_virtual_text(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.lsp.virtual_text = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_lsp_diagnostic_tooltip(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.lsp.diagnostic_tooltip = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_ghost_cut(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.editing.ghost_cut = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_quit_on_q(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.editing.quit_on_q = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_persistent_undo(
    settings: &mut UserSettings,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.undo.persistent = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_undo_dir(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Path(s) => {
            let s = s.trim();
            settings.undo.dir = if s.is_empty() {
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
        s.undo
            .dir
            .as_deref()
            .map_or_default(|p| p.display().to_string()),
    )
}

fn set_soft_wrap(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            settings.soft_wrap = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

const fn get_soft_wrap(s: &UserSettings) -> SettingValue {
    SettingValue::Bool(s.soft_wrap)
}

fn set_wrap_width(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Integer(n) => {
            settings.wrap_width = (n > 0).then_some(n);
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected integer".to_string(),
        )),
    }
}

fn get_wrap_width(s: &UserSettings) -> SettingValue {
    SettingValue::Integer(s.wrap_width.unwrap_or(0))
}

fn set_border_chars(settings: &mut UserSettings, value: &SettingValue) -> Result<(), SettingError> {
    let SettingValue::Text(text) = value else {
        return Err(SettingError::ValidationError("Expected text".to_string()));
    };
    let mut chars = text.chars();
    let (
        Some(top_left),
        Some(top_right),
        Some(bottom_left),
        Some(bottom_right),
        Some(horizontal),
        Some(vertical),
        None,
    ) = (
        chars.next(),
        chars.next(),
        chars.next(),
        chars.next(),
        chars.next(),
        chars.next(),
        chars.next(),
    )
    else {
        return Err(SettingError::ValidationError(format!(
            "Expected 6 border characters (top-left, top-right, bottom-left, bottom-right, horizontal, vertical), got '{text}'"
        )));
    };
    settings.default_border_chars = Some(BorderChars {
        top_left,
        top_right,
        bottom_left,
        bottom_right,
        horizontal,
        vertical,
    });
    Ok(())
}

fn get_border_chars(s: &UserSettings) -> SettingValue {
    let chars = s.default_border_chars.clone().unwrap_or_default();
    SettingValue::Text(
        [
            chars.top_left,
            chars.top_right,
            chars.bottom_left,
            chars.bottom_right,
            chars.horizontal,
            chars.vertical,
        ]
        .iter()
        .collect(),
    )
}

const SETTINGS: &[SettingDescriptor<UserSettings>] = &[
    SettingDescriptor {
        name: "command_line.borderstyle",
        aliases: &["clborderstyle"],
        description: "Style of the command line window border",
        ty: SettingType::Enum {
            variants: &["unicode", "ascii"],
        },
        set: set_border_style,
        get: get_border_style,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.border_chars",
        aliases: &[],
        description: "Command line window border characters: top-left, top-right, bottom-left, bottom-right, horizontal, vertical",
        ty: SettingType::Text,
        set: set_border_chars,
        get: get_border_chars,
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
        get: get_cmd_window_width_ratio,
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
        get: get_cmd_window_min_width,
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
        get: get_cmd_window_height,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.border",
        aliases: &[],
        description: "Show border around command line window",
        ty: SettingType::Boolean,
        set: set_cmd_window_border,
        get: get_cmd_window_border,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "command_line.reverse_video",
        aliases: &["clreverse"],
        description: "Use reverse video for command line window",
        ty: SettingType::Boolean,
        set: set_cmd_window_reverse_video,
        get: get_cmd_window_reverse_video,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "appearance.background",
        aliases: &["bg"],
        description: "Editor background color",
        ty: SettingType::Color,
        set: set_editor_bg,
        get: get_editor_bg,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "appearance.foreground",
        aliases: &["fg"],
        description: "Editor foreground color",
        ty: SettingType::Color,
        set: set_editor_fg,
        get: get_editor_fg,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "appearance.cursor_color",
        aliases: &["cursorcolor"],
        description: "Cursor block color in Normal mode",
        ty: SettingType::Color,
        set: set_cursor_color,
        get: get_cursor_color,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "appearance.selection_background",
        aliases: &["selbg"],
        description: "Visual selection background color (default: derived from the theme)",
        ty: SettingType::Color,
        set: set_selection_bg,
        get: get_selection_bg,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "appearance.selection_foreground",
        aliases: &["selfg"],
        description: "Visual selection text color (default: none, syntax colors show through)",
        ty: SettingType::Color,
        set: set_selection_fg,
        get: get_selection_fg,
        needs_full_redraw: true,
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
        get: get_cursor_speed,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "appearance.theme",
        aliases: &["colorscheme"],
        description: "Color theme",
        ty: SettingType::Enum {
            variants: crate::color::theme::Theme::NAMES,
        },
        set: set_theme,
        get: get_theme,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "status_line.show_filename",
        aliases: &[],
        description: "Show filename in status line",
        ty: SettingType::Boolean,
        set: set_show_filename,
        get: get_show_filename,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.reverse_video",
        aliases: &[],
        description: "Use reverse video for status line",
        ty: SettingType::Boolean,
        set: set_status_line_reverse_video,
        get: get_status_line_reverse_video,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.show_status_line",
        aliases: &[],
        description: "Show status line",
        ty: SettingType::Boolean,
        set: set_show_status_line,
        get: get_show_status_line,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "status_line.show_dirty",
        aliases: &[],
        description: "Show dirty indicator in status line",
        ty: SettingType::Boolean,
        set: set_show_dirty_indicator,
        get: get_show_dirty_indicator,
        needs_full_redraw: false,
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
        get: get_clipboard_size,
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
        get: get_poll_rate,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "editor.soft_wrap",
        aliases: &["softwrap"],
        description: "Soft-wrap documents that do not set their own wrap column",
        ty: SettingType::Boolean,
        set: set_soft_wrap,
        get: get_soft_wrap,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "editor.wrap_width",
        aliases: &[],
        description: "Soft-wrap column for documents that do not set their own (0 = window width)",
        ty: SettingType::Integer {
            min: None,
            max: None,
        },
        set: set_wrap_width,
        get: get_wrap_width,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "split.equalize_proportional",
        aliases: &["eqprop"],
        description: "When on, ^w= distributes space proportionally to leaf count; when off, each split gets 50/50",
        ty: SettingType::Boolean,
        set: set_equalize_proportional,
        get: get_equalize_proportional,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "lsp.debug_log",
        aliases: &["lspdebug"],
        description: "Show LSP protocol debug messages as notifications",
        ty: SettingType::Boolean,
        set: set_lsp_debug_log,
        get: get_lsp_debug_log,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "lsp.virtual_text",
        aliases: &["lspvt"],
        description: "Render LSP diagnostics as trailing end-of-line virtual text",
        ty: SettingType::Boolean,
        set: set_lsp_virtual_text,
        get: get_lsp_virtual_text,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "lsp.diagnostic_tooltip",
        aliases: &["lsptip"],
        description: "Show the diagnostic tooltip for the cursor line",
        ty: SettingType::Boolean,
        set: set_lsp_diagnostic_tooltip,
        get: get_lsp_diagnostic_tooltip,
        needs_full_redraw: true,
    },
    SettingDescriptor {
        name: "ghostcut",
        aliases: &["gc"],
        description: "Delete d-cuts immediately but grey them out in place until something resolves the paint, instead of removing them from the screen right away",
        ty: SettingType::Boolean,
        set: set_ghost_cut,
        get: get_ghost_cut,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "quitonq",
        aliases: &["qq"],
        description: "Make q in normal mode quit the editor immediately, discarding unsaved changes. When off, q behaves like :q",
        ty: SettingType::Boolean,
        set: set_quit_on_q,
        get: get_quit_on_q,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "undofile",
        aliases: &["udf"],
        description: "Persist undo history to disk so it survives editor restarts",
        ty: SettingType::Boolean,
        set: set_persistent_undo,
        get: get_persistent_undo,
        needs_full_redraw: false,
    },
    SettingDescriptor {
        name: "undodir",
        aliases: &["udir"],
        description: "Directory for persisted undo files (default: <config_dir>/undofiles)",
        ty: SettingType::Path,
        set: set_undo_dir,
        get: get_undo_dir,
        needs_full_redraw: false,
    },
];

#[must_use]
pub(crate) const fn create_settings_registry() -> SettingsRegistry<UserSettings> {
    SettingsRegistry::new(SETTINGS)
}
