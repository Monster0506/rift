use crate::command_line::settings::descriptor::{
    SettingDescriptor, SettingEffect, SettingError, SettingType, SettingValue,
};
use crate::command_line::settings::registry::SettingsRegistry;
use crate::document::LineEnding;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WrapMode {
    Off,
    Expr(String),
}

impl WrapMode {
    #[must_use]
    pub(crate) fn resolve(&self, terminal_width: usize) -> usize {
        match self {
            Self::Off => 0,
            Self::Expr(s) => crate::eval::eval(s, &|kw| {
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
pub(crate) struct DocumentOptions {
    /// Line ending character(s) to use for this document
    line_ending: LineEnding,

    /// Whether the file started with a UTF-8 BOM, written back on save
    has_bom: bool,

    /// Tab width in spaces (for display and expansion)
    tab_width: usize,
    /// Whether to expand tabs to spaces when inserting
    expand_tabs: bool,
    /// Whether to show line numbers for this document
    show_line_numbers: bool,
    /// Per-document wrap setting: None = inherit global, Some(mode) = override
    wrap: Option<WrapMode>,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        Self {
            line_ending: LineEnding::LF,
            has_bom: false,
            tab_width: 4,
            expand_tabs: true,
            show_line_numbers: true,
            wrap: Some(WrapMode::Expr("auto".to_string())),
        }
    }
}

impl DocumentOptions {
    #[must_use]
    pub(in crate::document) fn decoded(line_ending: LineEnding, has_bom: bool) -> Self {
        Self {
            line_ending,
            has_bom,
            ..Self::default()
        }
    }

    #[must_use]
    pub(in crate::document) fn without_line_numbers(self) -> Self {
        Self {
            show_line_numbers: false,
            ..self
        }
    }

    #[must_use]
    pub(in crate::document) fn without_wrap(self) -> Self {
        Self {
            wrap: Some(WrapMode::Off),
            ..self
        }
    }

    #[must_use]
    pub(crate) const fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    #[must_use]
    pub(crate) const fn has_bom(&self) -> bool {
        self.has_bom
    }

    #[must_use]
    pub(crate) const fn tab_width(&self) -> usize {
        self.tab_width
    }

    #[must_use]
    pub(crate) const fn expand_tabs(&self) -> bool {
        self.expand_tabs
    }

    #[must_use]
    pub(crate) const fn show_line_numbers(&self) -> bool {
        self.show_line_numbers
    }

    #[must_use]
    pub(crate) const fn wrap(&self) -> Option<&WrapMode> {
        self.wrap.as_ref()
    }
}

const fn get_tab_width(options: &DocumentOptions) -> SettingValue {
    SettingValue::Integer(options.tab_width)
}

const fn get_expand_tabs(options: &DocumentOptions) -> SettingValue {
    SettingValue::Bool(options.expand_tabs)
}

const fn get_number(options: &DocumentOptions) -> SettingValue {
    SettingValue::Bool(options.show_line_numbers)
}

fn get_line_ending(options: &DocumentOptions) -> SettingValue {
    SettingValue::Enum(options.line_ending.name().to_string())
}

fn set_tab_width(options: &mut DocumentOptions, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Integer(n) => {
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

fn set_expand_tabs(
    options: &mut DocumentOptions,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            options.expand_tabs = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_number(options: &mut DocumentOptions, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            options.show_line_numbers = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

fn set_line_ending(
    options: &mut DocumentOptions,
    value: &SettingValue,
) -> Result<(), SettingError> {
    match value {
        SettingValue::Enum(s) => match s.to_lowercase().as_str() {
            "lf" | "unix" => {
                options.line_ending = LineEnding::LF;
                Ok(())
            }
            "crlf" | "windows" | "dos" => {
                options.line_ending = LineEnding::Crlf;
                Ok(())
            }
            _ => Err(SettingError::ValidationError(format!(
                "Invalid line ending: {s}. Expected 'lf' or 'crlf'"
            ))),
        },
        _ => Err(SettingError::ValidationError(
            "Expected enum value for line ending".to_string(),
        )),
    }
}

fn get_wrap(options: &DocumentOptions) -> SettingValue {
    let expr = match &options.wrap {
        None | Some(WrapMode::Off) => return SettingValue::Integer(0),
        Some(WrapMode::Expr(s)) => s,
    };
    expr.trim()
        .parse::<usize>()
        .map_or_else(|_| SettingValue::Enum(expr.clone()), SettingValue::Integer)
}

fn set_wrap(options: &mut DocumentOptions, value: &SettingValue) -> Result<(), SettingError> {
    let expr = match value {
        &SettingValue::Integer(0) => {
            options.wrap = Some(WrapMode::Off);
            return Ok(());
        }
        &SettingValue::Integer(n) => n.to_string(),
        SettingValue::Enum(s) => s.clone(),
        _ => {
            return Err(SettingError::ValidationError(
                "Expected integer or expression".to_string(),
            ))
        }
    };
    options.wrap = Some(WrapMode::Expr(expr));
    Ok(())
}

fn set_bomb(options: &mut DocumentOptions, value: &SettingValue) -> Result<(), SettingError> {
    match value {
        &SettingValue::Bool(b) => {
            options.has_bom = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError(
            "Expected boolean".to_string(),
        )),
    }
}

const fn get_bomb(options: &DocumentOptions) -> SettingValue {
    SettingValue::Bool(options.has_bom)
}

/// Document-specific settings
/// `LOCAL_SETTINGS`
const DOCUMENT_SETTINGS: &[SettingDescriptor<DocumentOptions>] = &[
    SettingDescriptor {
        name: "number",
        aliases: &["nu", "show_line_numbers"],
        description: "Show line numbers",
        ty: SettingType::Boolean,
        set: set_number,
        get: get_number,
        effect: SettingEffect::Redraw,
    },
    SettingDescriptor {
        name: "line_ending",
        aliases: &["ff", "fileformat"], // mimicking vim's fileformat
        description: "Line ending format (lf/crlf)",
        ty: SettingType::Enum {
            variants: &["lf", "crlf", "unix", "dos", "windows"],
        },
        set: set_line_ending,
        get: get_line_ending,
        effect: SettingEffect::None,
    },
    SettingDescriptor {
        name: "expandtabs",
        aliases: &["et", "expand_tabs"],
        description: "Use spaces instead of tabs",
        ty: SettingType::Boolean,
        set: set_expand_tabs,
        get: get_expand_tabs,
        effect: SettingEffect::Redraw,
    },
    SettingDescriptor {
        name: "tabwidth",
        aliases: &["tw", "tab_width"],
        description: "Number of spaces per tab",
        ty: SettingType::Integer {
            min: Some(1),
            max: None,
        },
        set: set_tab_width,
        get: get_tab_width,
        effect: SettingEffect::Redraw,
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
        get: get_wrap,
        effect: SettingEffect::Redraw,
    },
    SettingDescriptor {
        name: "bomb",
        aliases: &["bom"],
        description: "Write a UTF-8 byte order mark when saving",
        ty: SettingType::Boolean,
        set: set_bomb,
        get: get_bomb,
        effect: SettingEffect::None,
    },
];

#[must_use]
pub(crate) const fn create_document_settings_registry() -> SettingsRegistry<DocumentOptions> {
    SettingsRegistry::new(DOCUMENT_SETTINGS)
}
