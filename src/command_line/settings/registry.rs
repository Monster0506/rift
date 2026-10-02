use super::descriptor::{SettingDescriptor, SettingError, SettingType, SettingValue};
use crate::command_line::commands::{CommandDef, CommandRegistry, ExecutionResult, MatchResult};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

static OPTION_REGISTRY_CACHE: LazyLock<Mutex<HashMap<usize, Arc<CommandRegistry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct SettingsRegistry<T: 'static> {
    settings: &'static [SettingDescriptor<T>],
}

impl<T> Clone for SettingsRegistry<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SettingsRegistry<T> {}

impl<T> SettingsRegistry<T> {
    #[must_use]
    pub const fn new(descriptors: &'static [SettingDescriptor<T>]) -> Self {
        SettingsRegistry {
            settings: descriptors,
        }
    }

    pub fn descriptors(&self) -> &[SettingDescriptor<T>] {
        self.settings
    }

    #[must_use]
    pub fn build_option_registry(&self) -> CommandRegistry {
        (*self.cached_option_registry()).clone()
    }

    fn cached_option_registry(&self) -> Arc<CommandRegistry> {
        let key = self.settings.as_ptr() as usize;
        let mut cache = OPTION_REGISTRY_CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache
            .entry(key)
            .or_insert_with(|| {
                let mut registry = CommandRegistry::new();
                for desc in self.settings {
                    let mut cmd_def = CommandDef::new(desc.name);
                    for alias in desc.aliases {
                        cmd_def = cmd_def.with_alias(*alias);
                    }
                    registry = registry.register(cmd_def);
                }
                Arc::new(registry)
            })
            .clone()
    }

    #[must_use]
    pub fn find_descriptor(&self, name: &str) -> Option<&SettingDescriptor<T>> {
        let matched_name = match self.cached_option_registry().match_command(name) {
            MatchResult::Exact(n) | MatchResult::Prefix(n) => n,
            MatchResult::Ambiguous { .. } | MatchResult::Unknown(_) => return None,
        };
        self.settings.iter().find(|d| d.name == matched_name)
    }

    #[must_use]
    pub fn get_setting(&self, name: &str, target: &T) -> Option<SettingValue> {
        let desc = self.find_descriptor(name)?;
        desc.get.map(|getter| getter(target))
    }

    pub(crate) fn parse_value(ty: &SettingType, value: &str) -> Result<SettingValue, SettingError> {
        match ty {
            SettingType::Boolean => {
                let val_lower = value.to_lowercase();
                match val_lower.as_str() {
                    "true" | "1" | "on" | "yes" => Ok(SettingValue::Bool(true)),
                    "false" | "0" | "off" | "no" => Ok(SettingValue::Bool(false)),
                    _ => Err(SettingError::ParseError(format!(
                        "Invalid boolean value: {value}"
                    ))),
                }
            }
            SettingType::Integer { min, max } => {
                let val = value.parse::<usize>().map_err(|_| {
                    SettingError::ParseError(format!("Invalid integer value: {value}"))
                })?;

                if let Some(min_val) = min {
                    if val < *min_val {
                        return Err(SettingError::ValidationError(format!(
                            "Value {val} is below minimum {min_val}"
                        )));
                    }
                }
                if let Some(max_val) = max {
                    if val > *max_val {
                        return Err(SettingError::ValidationError(format!(
                            "Value {val} is above maximum {max_val}"
                        )));
                    }
                }
                Ok(SettingValue::Integer(val))
            }
            SettingType::Float { min, max } => {
                let val = value.parse::<f64>().map_err(|_| {
                    SettingError::ParseError(format!("Invalid float value: {value}"))
                })?;

                if let Some(min_val) = min {
                    if val < *min_val {
                        return Err(SettingError::ValidationError(format!(
                            "Value {val} is below minimum {min_val}"
                        )));
                    }
                }
                if let Some(max_val) = max {
                    if val > *max_val {
                        return Err(SettingError::ValidationError(format!(
                            "Value {val} is above maximum {max_val}"
                        )));
                    }
                }
                Ok(SettingValue::Float(val))
            }
            SettingType::Enum { variants } => {
                let val_lower = value.to_lowercase();
                if let Some(canonical) = variants.iter().find(|v| v.to_lowercase() == val_lower) {
                    Ok(SettingValue::Enum(canonical.to_string()))
                } else {
                    Err(SettingError::ParseError(format!(
                        "Invalid enum value: {value}. Valid values: {variants:?}"
                    )))
                }
            }
            SettingType::Color => Self::parse_color(value),
            SettingType::IntegerOrKeyword { min, max, keywords } => {
                let result = crate::eval::eval(value, &|kw| {
                    if keywords.iter().any(|k| k.to_lowercase() == kw) {
                        Some(1)
                    } else {
                        None
                    }
                });
                let eval_result = result.map_err(SettingError::ParseError)?;
                if let Some(min_val) = min {
                    if eval_result < *min_val {
                        return Err(SettingError::ValidationError(format!(
                            "Value is below minimum {min_val}"
                        )));
                    }
                }
                if let Some(max_val) = max {
                    if eval_result > *max_val {
                        return Err(SettingError::ValidationError(format!(
                            "Value is above maximum {max_val}"
                        )));
                    }
                }
                if value.trim().parse::<usize>().is_ok() {
                    return Ok(SettingValue::Integer(eval_result));
                }
                Ok(SettingValue::Enum(value.to_string()))
            }
            SettingType::Path => Ok(SettingValue::Path(value.to_string())),
        }
    }

    fn parse_color(value: &str) -> Result<SettingValue, SettingError> {
        use crate::color::Color;
        let val_lower = value.to_lowercase().trim().to_string();

        if val_lower == "reset" || val_lower == "default" || val_lower == "none" {
            return Ok(SettingValue::Color(Color::Reset));
        }

        if val_lower.starts_with("rgb(") && val_lower.ends_with(')') {
            let rgb_str = &val_lower[4..val_lower.len() - 1];
            let parts: Vec<&str> = rgb_str.split(',').map(str::trim).collect();
            if parts.len() == 3 {
                let r = parts[0].parse::<u8>().map_err(|_| {
                    SettingError::ParseError(format!("Invalid RGB red value: {}", parts[0]))
                })?;
                let g = parts[1].parse::<u8>().map_err(|_| {
                    SettingError::ParseError(format!("Invalid RGB green value: {}", parts[1]))
                })?;
                let b = parts[2].parse::<u8>().map_err(|_| {
                    SettingError::ParseError(format!("Invalid RGB blue value: {}", parts[2]))
                })?;
                return Ok(SettingValue::Color(Color::Rgb { r, g, b }));
            }
        }

        if let Some(hex) = val_lower.strip_prefix("#") {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|_| SettingError::ParseError(format!("Invalid hex color: {value}")))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|_| SettingError::ParseError(format!("Invalid hex color: {value}")))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|_| SettingError::ParseError(format!("Invalid hex color: {value}")))?;
                return Ok(SettingValue::Color(Color::Rgb { r, g, b }));
            } else if hex.len() == 3 {
                let r = u8::from_str_radix(&hex[0..1], 16)
                    .map_err(|_| SettingError::ParseError(format!("Invalid hex color: {value}")))?;
                let g = u8::from_str_radix(&hex[1..2], 16)
                    .map_err(|_| SettingError::ParseError(format!("Invalid hex color: {value}")))?;
                let b = u8::from_str_radix(&hex[2..3], 16)
                    .map_err(|_| SettingError::ParseError(format!("Invalid hex color: {value}")))?;
                let r = (r << 4) | r;
                let g = (g << 4) | g;
                let b = (b << 4) | b;
                return Ok(SettingValue::Color(Color::Rgb { r, g, b }));
            }
        }

        if val_lower.starts_with("ansi256(") && val_lower.ends_with(')') {
            let num_str = &val_lower[8..val_lower.len() - 1];
            let n = num_str.parse::<u8>().map_err(|_| {
                SettingError::ParseError(format!("Invalid ANSI256 color index: {num_str}"))
            })?;
            return Ok(SettingValue::Color(Color::Ansi256(n)));
        }

        if let Ok(n) = val_lower.parse::<u8>() {
            return Ok(SettingValue::Color(Color::Ansi256(n)));
        }

        let color = match val_lower.as_str() {
            "black" => Color::Black,
            "darkgrey" | "dark_grey" => Color::DarkGrey,
            "red" => Color::Red,
            "darkred" | "dark_red" => Color::DarkRed,
            "green" => Color::Green,
            "darkgreen" | "dark_green" => Color::DarkGreen,
            "yellow" => Color::Yellow,
            "darkyellow" | "dark_yellow" => Color::DarkYellow,
            "blue" => Color::Blue,
            "darkblue" | "dark_blue" => Color::DarkBlue,
            "magenta" => Color::Magenta,
            "darkmagenta" | "dark_magenta" => Color::DarkMagenta,
            "cyan" => Color::Cyan,
            "darkcyan" | "dark_cyan" => Color::DarkCyan,
            "white" => Color::White,
            "grey" | "gray" => Color::Grey,
            _ => {
                return Err(SettingError::ParseError(format!(
                    "Unknown color name: {value}. Use color names, rgb(r,g,b), #hex, or ansi256(n)"
                )))
            }
        };

        Ok(SettingValue::Color(color))
    }

    pub fn execute_setting(
        &self,
        name: &str,
        value: Option<String>,
        target: &mut T,
        error_handler: &mut dyn FnMut(RiftError),
    ) -> ExecutionResult {
        let matched_name = match self.cached_option_registry().match_command(name) {
            MatchResult::Exact(n) | MatchResult::Prefix(n) => n,
            MatchResult::Ambiguous { prefix, matches } => {
                let matches_str = matches.join(", ");
                error_handler(RiftError {
                    severity: ErrorSeverity::Error,
                    kind: ErrorType::Settings,
                    code: "AMBIGUOUS_SETTING".to_string(),
                    message: format!("Ambiguous option '{prefix}': matches {matches_str}"),
                });
                return ExecutionResult::Failure;
            }
            MatchResult::Unknown(_) => {
                error_handler(RiftError {
                    severity: ErrorSeverity::Error,
                    kind: ErrorType::Settings,
                    code: "UNKNOWN_SETTING".to_string(),
                    message: format!("Unknown option: {name}"),
                });
                return ExecutionResult::Failure;
            }
        };

        let desc = match self.settings.iter().find(|d| d.name == matched_name) {
            Some(d) => d,
            None => {
                error_handler(RiftError {
                    severity: ErrorSeverity::Error,
                    kind: ErrorType::Settings,
                    code: "UNKNOWN_SETTING".to_string(),
                    message: format!("Unknown option: {name}"),
                });
                return ExecutionResult::Failure;
            }
        };

        let value_str = match value.as_ref() {
            Some(v) => v,
            None => {
                error_handler(RiftError {
                    severity: ErrorSeverity::Error,
                    kind: ErrorType::Settings,
                    code: "MISSING_SETTING_VALUE".to_string(),
                    message: "Missing value".to_string(),
                });
                return ExecutionResult::Failure;
            }
        };

        let typed_value = match Self::parse_value(&desc.ty, value_str) {
            Ok(v) => v,
            Err(e) => {
                error_handler(e.into());
                return ExecutionResult::Failure;
            }
        };

        match (desc.set)(target, typed_value) {
            Ok(()) => {
                if desc.needs_full_redraw {
                    ExecutionResult::Redraw
                } else {
                    ExecutionResult::Success
                }
            }
            Err(e) => {
                error_handler(e.into());
                ExecutionResult::Failure
            }
        }
    }
}
