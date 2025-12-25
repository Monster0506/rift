//! Settings registry
//! Registry that holds setting descriptors and provides execution

use super::descriptor::{SettingDescriptor, SettingError, SettingType, SettingValue};
use crate::command_line::executor::ExecutionResult;
use crate::command_line::registry::{CommandDef, CommandRegistry, MatchResult};
use crate::state::UserSettings;

/// Settings registry
///
/// Holds static setting descriptors and provides:
/// - Option registry building (for parser)
/// - Setting execution (for executor)
#[derive(Clone, Copy)]
pub struct SettingsRegistry {
    /// Static array of setting descriptors
    settings: &'static [SettingDescriptor],
}

impl SettingsRegistry {
    /// Create a new registry from static descriptors
    #[must_use]
    pub const fn new(descriptors: &'static [SettingDescriptor]) -> Self {
        SettingsRegistry {
            settings: descriptors,
        }
    }

    /// Build `CommandRegistry` for option name matching
    ///
    /// Generates a `CommandRegistry` from all setting descriptors,
    /// enabling prefix matching and alias resolution for option names.
    #[must_use]
    pub fn build_option_registry(&self) -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        for desc in self.settings {
            let mut cmd_def = CommandDef::new(desc.name);
            for alias in desc.aliases {
                cmd_def = cmd_def.with_alias(*alias);
            }
            registry = registry.register(cmd_def);
        }
        registry
    }

    /// Parse string value to `SettingValue` using `SettingType`
    ///
    /// Handles parsing and validation according to the setting type.
    /// Returns typed `SettingValue` or structured error.
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
                // Find canonical variant (case-insensitive match)
                if let Some(canonical) = variants.iter().find(|v| v.to_lowercase() == val_lower) {
                    Ok(SettingValue::Enum(canonical.to_string()))
                } else {
                    Err(SettingError::ParseError(format!(
                        "Invalid enum value: {value}. Valid values: {variants:?}"
                    )))
                }
            }
            SettingType::Color => Self::parse_color(value),
        }
    }

    /// Parse a color string to a Color value
    /// Supports:
    /// - Color names: black, red, green, yellow, blue, magenta, cyan, white, grey, darkred, etc.
    /// - RGB: rgb(255,128,64) or #ff8040
    /// - 256-color: ansi256(100) or just 100
    /// - Reset: reset, default, none
    fn parse_color(value: &str) -> Result<SettingValue, SettingError> {
        use crate::color::Color;
        let val_lower = value.to_lowercase().trim().to_string();

        // Handle reset/default/none
        if val_lower == "reset" || val_lower == "default" || val_lower == "none" {
            return Ok(SettingValue::Color(Color::Reset));
        }

        // Handle RGB format: rgb(255,128,64) or #ff8040
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

        // Handle hex format: #ff8040 or #fff
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
                // Short hex format: #fff -> #ffffff
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

        // Handle ansi256 format: ansi256(100) or just 100
        if val_lower.starts_with("ansi256(") && val_lower.ends_with(')') {
            let num_str = &val_lower[8..val_lower.len() - 1];
            let n = num_str.parse::<u8>().map_err(|_| {
                SettingError::ParseError(format!("Invalid ANSI256 color index: {num_str}"))
            })?;
            return Ok(SettingValue::Color(Color::Ansi256(n)));
        }

        // Try parsing as a number (256-color index)
        if let Ok(n) = val_lower.parse::<u8>() {
            return Ok(SettingValue::Color(Color::Ansi256(n)));
        }

        // Handle color names
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

    /// Execute a setting by name with string value
    ///
    /// Flow:
    /// 1. Resolve option name using registry matching (handles aliases, prefixes)
    /// 2. Find descriptor by matched name
    /// 3. Parse string value to `SettingValue` using `SettingType`
    /// 4. Call setter function with typed value
    /// 5. Return `ExecutionResult`
    pub fn execute_setting(
        &self,
        name: &str,
        value: Option<String>,
        settings: &mut UserSettings,
    ) -> ExecutionResult {
        // Build registry for name matching
        let registry = self.build_option_registry();

        // Resolve option name (handles aliases, prefixes, ambiguity)
        let matched_name = match registry.match_command(name) {
            MatchResult::Exact(n) | MatchResult::Prefix(n) => n,
            MatchResult::Ambiguous { prefix, matches } => {
                let matches_str = matches.join(", ");
                return ExecutionResult::Error(format!(
                    "Ambiguous option '{prefix}': matches {matches_str}"
                ));
            }
            MatchResult::Unknown(_) => {
                return ExecutionResult::Error(format!("Unknown option: {name}"));
            }
        };

        // Find descriptor by matched name
        let desc = match self.settings.iter().find(|d| d.name == matched_name) {
            Some(d) => d,
            None => return ExecutionResult::Error(format!("Unknown option: {name}")),
        };

        // Parse value
        let value_str = match value.as_ref() {
            Some(v) => v,
            None => return ExecutionResult::Error("Missing value".to_string()),
        };

        let typed_value = match Self::parse_value(&desc.ty, value_str) {
            Ok(v) => v,
            Err(e) => return ExecutionResult::Error(e.to_string()),
        };

        // Apply setter
        match (desc.set)(settings, typed_value) {
            Ok(()) => ExecutionResult::Success,
            Err(e) => ExecutionResult::Error(e.to_string()),
        }
    }
}
