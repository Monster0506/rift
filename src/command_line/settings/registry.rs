//! Settings registry
//! Registry that holds setting descriptors and provides execution

use crate::command_line::registry::{CommandRegistry, CommandDef, MatchResult};
use crate::command_line::executor::ExecutionResult;
use crate::state::UserSettings;
use super::descriptor::{SettingDescriptor, SettingType, SettingValue, SettingError};

/// Settings registry
/// 
/// Holds static setting descriptors and provides:
/// - Option registry building (for parser)
/// - Setting execution (for executor)
pub struct SettingsRegistry {
    /// Static array of setting descriptors
    settings: &'static [SettingDescriptor],
}

impl SettingsRegistry {
    /// Create a new registry from static descriptors
    pub const fn new(descriptors: &'static [SettingDescriptor]) -> Self {
        SettingsRegistry {
            settings: descriptors,
        }
    }
    
    /// Build CommandRegistry for option name matching
    /// 
    /// Generates a CommandRegistry from all setting descriptors,
    /// enabling prefix matching and alias resolution for option names.
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
    
    /// Parse string value to SettingValue using SettingType
    /// 
    /// Handles parsing and validation according to the setting type.
    /// Returns typed SettingValue or structured error.
    pub(crate) fn parse_value(ty: &SettingType, value: &str) -> Result<SettingValue, SettingError> {
        match ty {
            SettingType::Boolean => {
                let val_lower = value.to_lowercase();
                match val_lower.as_str() {
                    "true" | "1" | "on" | "yes" => Ok(SettingValue::Bool(true)),
                    "false" | "0" | "off" | "no" => Ok(SettingValue::Bool(false)),
                    _ => Err(SettingError::ParseError(format!("Invalid boolean value: {}", value))),
                }
            }
            SettingType::Integer { min, max } => {
                let val = value.parse::<usize>()
                    .map_err(|_| SettingError::ParseError(format!("Invalid integer value: {}", value)))?;
                
                if let Some(min_val) = min {
                    if val < *min_val {
                        return Err(SettingError::ValidationError(
                            format!("Value {} is below minimum {}", val, min_val)
                        ));
                    }
                }
                if let Some(max_val) = max {
                    if val > *max_val {
                        return Err(SettingError::ValidationError(
                            format!("Value {} is above maximum {}", val, max_val)
                        ));
                    }
                }
                Ok(SettingValue::Integer(val))
            }
            SettingType::Float { min, max } => {
                let val = value.parse::<f64>()
                    .map_err(|_| SettingError::ParseError(format!("Invalid float value: {}", value)))?;
                
                if let Some(min_val) = min {
                    if val < *min_val {
                        return Err(SettingError::ValidationError(
                            format!("Value {} is below minimum {}", val, min_val)
                        ));
                    }
                }
                if let Some(max_val) = max {
                    if val > *max_val {
                        return Err(SettingError::ValidationError(
                            format!("Value {} is above maximum {}", val, max_val)
                        ));
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
                    Err(SettingError::ParseError(
                        format!("Invalid enum value: {}. Valid values: {:?}", value, variants)
                    ))
                }
            }
        }
    }
    
    /// Execute a setting by name with string value
    /// 
    /// Flow:
    /// 1. Resolve option name using registry matching (handles aliases, prefixes)
    /// 2. Find descriptor by matched name
    /// 3. Parse string value to SettingValue using SettingType
    /// 4. Call setter function with typed value
    /// 5. Return ExecutionResult
    pub fn execute_setting(&self, name: &str, value: Option<String>, 
                          settings: &mut UserSettings) -> ExecutionResult {
        // Build registry for name matching
        let registry = self.build_option_registry();
        
        // Resolve option name (handles aliases, prefixes, ambiguity)
        let matched_name = match registry.match_command(name) {
            MatchResult::Exact(n) | MatchResult::Prefix(n) => n,
            MatchResult::Ambiguous { prefix, matches } => {
                let matches_str = matches.join(", ");
                return ExecutionResult::Error(format!(
                    "Ambiguous option '{}': matches {}",
                    prefix, matches_str
                ));
            }
            MatchResult::Unknown(_) => {
                return ExecutionResult::Error(format!("Unknown option: {}", name));
            }
        };
        
        // Find descriptor by matched name
        let desc = match self.settings.iter().find(|d| d.name == matched_name) {
            Some(d) => d,
            None => return ExecutionResult::Error(format!("Unknown option: {}", name)),
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

