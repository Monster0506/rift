//! Tests for settings registry

use crate::command_line::settings::descriptor::{SettingDescriptor, SettingType, SettingValue, SettingError};
use crate::command_line::settings::registry::SettingsRegistry;
use crate::command_line::settings::create_settings_registry;
use crate::state::UserSettings;

// Test setters for various types
fn set_expand_tabs(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Bool(b) => {
            settings.expand_tabs = b;
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected boolean".to_string())),
    }
}

fn set_tab_width(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Integer(n) => {
            if n == 0 {
                return Err(SettingError::ValidationError("tabwidth must be greater than 0".to_string()));
            }
            settings.tab_width = n;
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected integer".to_string())),
    }
}

fn set_width_ratio(settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Float(f) => {
            if f < 0.0 || f > 1.0 {
                return Err(SettingError::ValidationError("width_ratio must be between 0.0 and 1.0".to_string()));
            }
            settings.command_line_window.width_ratio = f;
            Ok(())
        }
        _ => Err(SettingError::ValidationError("Expected float".to_string())),
    }
}

fn set_border_style(_settings: &mut UserSettings, value: SettingValue) -> Result<(), SettingError> {
    match value {
        SettingValue::Enum(style) => {
            match style.as_str() {
                "unicode" | "ascii" | "none" => {
                    // Just verify it's a valid enum value
                    Ok(())
                }
                _ => Err(SettingError::ValidationError(format!("Unknown border style: {}", style))),
            }
        }
        _ => Err(SettingError::ValidationError("Expected enum".to_string())),
    }
}

const TEST_SETTINGS: &[SettingDescriptor] = &[
    SettingDescriptor {
        name: "expandtabs",
        aliases: &["et"],
        ty: SettingType::Boolean,
        set: set_expand_tabs,
    },
    SettingDescriptor {
        name: "tabwidth",
        aliases: &["tw"],
        ty: SettingType::Integer { min: Some(1), max: None },
        set: set_tab_width,
    },
    SettingDescriptor {
        name: "command_line_window.width_ratio",
        aliases: &["cmdwidth"],
        ty: SettingType::Float { min: Some(0.0), max: Some(1.0) },
        set: set_width_ratio,
    },
    SettingDescriptor {
        name: "borderstyle",
        aliases: &["bs"],
        ty: SettingType::Enum { variants: &["unicode", "ascii", "none"] },
        set: set_border_style,
    },
];

fn create_test_registry() -> SettingsRegistry {
    SettingsRegistry::new(TEST_SETTINGS)
}

#[test]
fn test_setting_value_debug() {
    assert_eq!(format!("{:?}", SettingValue::Bool(true)), "Bool(true)");
    assert_eq!(format!("{:?}", SettingValue::Integer(42)), "Integer(42)");
    assert_eq!(format!("{:?}", SettingValue::Float(3.14)), "Float(3.14)");
    assert_eq!(format!("{:?}", SettingValue::Enum("test".to_string())), "Enum(\"test\")");
}

#[test]
fn test_setting_error_display() {
    assert_eq!(
        SettingError::ParseError("invalid".to_string()).to_string(),
        "Parse error: invalid"
    );
    assert_eq!(
        SettingError::ValidationError("out of range".to_string()).to_string(),
        "Validation error: out of range"
    );
    assert_eq!(
        SettingError::UnknownOption("foo".to_string()).to_string(),
        "Unknown option: foo"
    );
}

#[test]
fn test_parse_boolean_true() {
    let desc = &TEST_SETTINGS[0]; // expandtabs
    
    let result = SettingsRegistry::parse_value(&desc.ty, "true");
    assert_eq!(result, Ok(SettingValue::Bool(true)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "1");
    assert_eq!(result, Ok(SettingValue::Bool(true)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "on");
    assert_eq!(result, Ok(SettingValue::Bool(true)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "yes");
    assert_eq!(result, Ok(SettingValue::Bool(true)));
    
    // Case insensitive
    let result = SettingsRegistry::parse_value(&desc.ty, "TRUE");
    assert_eq!(result, Ok(SettingValue::Bool(true)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "ON");
    assert_eq!(result, Ok(SettingValue::Bool(true)));
}

#[test]
fn test_parse_boolean_false() {
    let desc = &TEST_SETTINGS[0]; // expandtabs
    
    let result = SettingsRegistry::parse_value(&desc.ty, "false");
    assert_eq!(result, Ok(SettingValue::Bool(false)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "0");
    assert_eq!(result, Ok(SettingValue::Bool(false)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "off");
    assert_eq!(result, Ok(SettingValue::Bool(false)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "no");
    assert_eq!(result, Ok(SettingValue::Bool(false)));
    
    // Case insensitive
    let result = SettingsRegistry::parse_value(&desc.ty, "FALSE");
    assert_eq!(result, Ok(SettingValue::Bool(false)));
}

#[test]
fn test_parse_boolean_invalid() {
    let desc = &TEST_SETTINGS[0]; // expandtabs
    
    let result = SettingsRegistry::parse_value(&desc.ty, "maybe");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "2");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
}

#[test]
fn test_parse_integer_valid() {
    let desc = &TEST_SETTINGS[1]; // tabwidth
    
    let result = SettingsRegistry::parse_value(&desc.ty, "42");
    assert_eq!(result, Ok(SettingValue::Integer(42)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "1");
    assert_eq!(result, Ok(SettingValue::Integer(1)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "1000");
    assert_eq!(result, Ok(SettingValue::Integer(1000)));
}

#[test]
fn test_parse_integer_with_bounds() {
    let desc = &TEST_SETTINGS[1]; // tabwidth (min: 1)
    
    // Below minimum
    let result = SettingsRegistry::parse_value(&desc.ty, "0");
    assert!(matches!(result, Err(SettingError::ValidationError(_))));
    
    // At minimum
    let result = SettingsRegistry::parse_value(&desc.ty, "1");
    assert_eq!(result, Ok(SettingValue::Integer(1)));
    
    // Above minimum
    let result = SettingsRegistry::parse_value(&desc.ty, "8");
    assert_eq!(result, Ok(SettingValue::Integer(8)));
}

#[test]
fn test_parse_integer_invalid() {
    let desc = &TEST_SETTINGS[1]; // tabwidth
    
    let result = SettingsRegistry::parse_value(&desc.ty, "not_a_number");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "3.14");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
}

#[test]
fn test_parse_float_valid() {
    let desc = &TEST_SETTINGS[2]; // width_ratio
    
    let result = SettingsRegistry::parse_value(&desc.ty, "0.5");
    assert_eq!(result, Ok(SettingValue::Float(0.5)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "0.0");
    assert_eq!(result, Ok(SettingValue::Float(0.0)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "1.0");
    assert_eq!(result, Ok(SettingValue::Float(1.0)));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "0.75");
    assert_eq!(result, Ok(SettingValue::Float(0.75)));
}

#[test]
fn test_parse_float_with_bounds() {
    let desc = &TEST_SETTINGS[2]; // width_ratio (min: 0.0, max: 1.0)
    
    // Below minimum
    let result = SettingsRegistry::parse_value(&desc.ty, "-0.1");
    assert!(matches!(result, Err(SettingError::ValidationError(_))));
    
    // At minimum
    let result = SettingsRegistry::parse_value(&desc.ty, "0.0");
    assert_eq!(result, Ok(SettingValue::Float(0.0)));
    
    // In range
    let result = SettingsRegistry::parse_value(&desc.ty, "0.6");
    assert_eq!(result, Ok(SettingValue::Float(0.6)));
    
    // At maximum
    let result = SettingsRegistry::parse_value(&desc.ty, "1.0");
    assert_eq!(result, Ok(SettingValue::Float(1.0)));
    
    // Above maximum
    let result = SettingsRegistry::parse_value(&desc.ty, "1.1");
    assert!(matches!(result, Err(SettingError::ValidationError(_))));
}

#[test]
fn test_parse_float_invalid() {
    let desc = &TEST_SETTINGS[2]; // width_ratio
    
    let result = SettingsRegistry::parse_value(&desc.ty, "not_a_float");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
}

#[test]
fn test_parse_enum_valid() {
    let desc = &TEST_SETTINGS[3]; // borderstyle
    
    let result = SettingsRegistry::parse_value(&desc.ty, "unicode");
    assert_eq!(result, Ok(SettingValue::Enum("unicode".to_string())));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "ascii");
    assert_eq!(result, Ok(SettingValue::Enum("ascii".to_string())));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "none");
    assert_eq!(result, Ok(SettingValue::Enum("none".to_string())));
}

#[test]
fn test_parse_enum_case_insensitive() {
    let desc = &TEST_SETTINGS[3]; // borderstyle
    
    // Case insensitive matching, but returns canonical form
    let result = SettingsRegistry::parse_value(&desc.ty, "UNICODE");
    assert_eq!(result, Ok(SettingValue::Enum("unicode".to_string())));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "Unicode");
    assert_eq!(result, Ok(SettingValue::Enum("unicode".to_string())));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "ASCII");
    assert_eq!(result, Ok(SettingValue::Enum("ascii".to_string())));
}

#[test]
fn test_parse_enum_invalid() {
    let desc = &TEST_SETTINGS[3]; // borderstyle
    
    let result = SettingsRegistry::parse_value(&desc.ty, "invalid");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "custom");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
    
    let result = SettingsRegistry::parse_value(&desc.ty, "");
    assert!(matches!(result, Err(SettingError::ParseError(_))));
}

#[test]
fn test_build_option_registry() {
    let registry = create_test_registry();
    let option_registry = registry.build_option_registry();
    
    // Test exact match
    use crate::command_line::registry::MatchResult;
    match option_registry.match_command("expandtabs") {
        MatchResult::Exact(name) => assert_eq!(name, "expandtabs"),
        _ => panic!("Expected exact match"),
    }
    
    // Test alias match
    match option_registry.match_command("et") {
        MatchResult::Exact(name) => assert_eq!(name, "expandtabs"),
        _ => panic!("Expected alias match"),
    }
    
    // Test prefix match
    match option_registry.match_command("expa") {
        MatchResult::Prefix(name) => assert_eq!(name, "expandtabs"),
        _ => panic!("Expected prefix match"),
    }
    
    // Test nested setting
    match option_registry.match_command("command_line_window.width_ratio") {
        MatchResult::Exact(name) => assert_eq!(name, "command_line_window.width_ratio"),
        _ => panic!("Expected exact match for nested setting"),
    }
    
    // Test nested setting alias
    match option_registry.match_command("cmdwidth") {
        MatchResult::Exact(name) => assert_eq!(name, "command_line_window.width_ratio"),
        _ => panic!("Expected alias match for nested setting"),
    }
}

#[test]
fn test_execute_setting_boolean() {
    let registry = create_test_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("expandtabs", Some("false".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.expand_tabs, false);
    
    let result = registry.execute_setting("expandtabs", Some("true".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.expand_tabs, true);
    
    let result = registry.execute_setting("et", Some("false".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.expand_tabs, false);
}

#[test]
fn test_execute_setting_integer() {
    let registry = create_test_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("tabwidth", Some("4".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.tab_width, 4);
    
    let result = registry.execute_setting("tw", Some("8".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.tab_width, 8);
    
    let result = registry.execute_setting("tabw", Some("2".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.tab_width, 2);
}

#[test]
fn test_execute_setting_float() {
    let registry = create_test_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("command_line_window.width_ratio", Some("0.6".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.command_line_window.width_ratio, 0.6);
    
    let result = registry.execute_setting("cmdwidth", Some("0.8".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.command_line_window.width_ratio, 0.8);
}

#[test]
fn test_execute_setting_enum() {
    let registry = create_test_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("borderstyle", Some("unicode".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    
    let result = registry.execute_setting("borderstyle", Some("ASCII".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    
    let result = registry.execute_setting("bs", Some("none".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
}

#[test]
fn test_execute_setting_missing_value() {
    let registry = create_test_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("expandtabs", None, &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
}

#[test]
fn test_execute_setting_unknown_option() {
    let registry = create_test_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("unknown_option", Some("value".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
}

#[test]
fn test_execute_setting_invalid_value() {
    let registry = create_test_registry();
    let mut settings = UserSettings::new();
    
    // Invalid boolean
    let result = registry.execute_setting("expandtabs", Some("maybe".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
    
    // Invalid integer
    let result = registry.execute_setting("tabwidth", Some("not_a_number".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
    
    // Invalid enum
    let result = registry.execute_setting("borderstyle", Some("invalid".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
}

#[test]
fn test_execute_setting_validation_error() {
    let registry = create_test_registry();
    let mut settings = UserSettings::new();
    
    // tabwidth below minimum (0)
    let result = registry.execute_setting("tabwidth", Some("0".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
    
    // width_ratio out of range
    let result = registry.execute_setting("command_line_window.width_ratio", Some("1.5".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
    
    let result = registry.execute_setting("command_line_window.width_ratio", Some("-0.1".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
}

#[test]
fn test_execute_setting_ambiguous() {
    // Create registry with ambiguous options
    const AMBIGUOUS_SETTINGS: &[SettingDescriptor] = &[
        SettingDescriptor {
            name: "expandtabs",
            aliases: &[],
            ty: SettingType::Boolean,
            set: set_expand_tabs,
        },
        SettingDescriptor {
            name: "expandspaces",
            aliases: &[],
            ty: SettingType::Boolean,
            set: set_expand_tabs,
        },
    ];
    
    let registry = SettingsRegistry::new(AMBIGUOUS_SETTINGS);
    let mut settings = UserSettings::new();
    
    // "expa" is ambiguous
    let result = registry.execute_setting("expa", Some("true".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
    
    // But "expandtabs" is unambiguous
    let result = registry.execute_setting("expandtabs", Some("true".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
}

#[test]
fn test_execute_setting_theme_light() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("theme", Some("light".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("light".to_string()));
    assert_eq!(settings.editor_bg, Some(crate::color::Theme::light().background));
    assert_eq!(settings.editor_fg, Some(crate::color::Theme::light().foreground));
}

#[test]
fn test_execute_setting_theme_dark() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("theme", Some("dark".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("dark".to_string()));
    assert_eq!(settings.editor_bg, Some(crate::color::Theme::dark().background));
    assert_eq!(settings.editor_fg, Some(crate::color::Theme::dark().foreground));
}

#[test]
fn test_execute_setting_theme_gruvbox() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("theme", Some("gruvbox".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("gruvbox".to_string()));
    assert_eq!(settings.editor_bg, Some(crate::color::Theme::gruvbox().background));
    assert_eq!(settings.editor_fg, Some(crate::color::Theme::gruvbox().foreground));
}

#[test]
fn test_execute_setting_theme_nordic() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("theme", Some("nordic".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("nordic".to_string()));
    assert_eq!(settings.editor_bg, Some(crate::color::Theme::nordic().background));
    assert_eq!(settings.editor_fg, Some(crate::color::Theme::nordic().foreground));
}

#[test]
fn test_execute_setting_theme_nord_alias() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("theme", Some("nord".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("nordic".to_string()));
}

#[test]
fn test_execute_setting_theme_case_insensitive() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("theme", Some("LIGHT".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("light".to_string()));
    
    let result = registry.execute_setting("theme", Some("Dark".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("dark".to_string()));
}

#[test]
fn test_execute_setting_theme_unknown() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    let result = registry.execute_setting("theme", Some("unknown_theme".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Error(_)));
}

#[test]
fn test_execute_setting_theme_overwrites_previous() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    // Apply light theme
    let result = registry.execute_setting("theme", Some("light".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("light".to_string()));
    let light_bg = settings.editor_bg;
    
    // Apply dark theme - should overwrite
    let result = registry.execute_setting("theme", Some("dark".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("dark".to_string()));
    assert_ne!(settings.editor_bg, light_bg);
    assert_eq!(settings.editor_bg, Some(crate::color::Theme::dark().background));
}

#[test]
fn test_execute_setting_theme_alias_colorscheme() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    // Test alias "colorscheme"
    let result = registry.execute_setting("colorscheme", Some("light".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("light".to_string()));
}

#[test]
fn test_execute_setting_theme_alias_colors() {
    let registry = create_settings_registry();
    let mut settings = UserSettings::new();
    
    // Test alias "colors"
    let result = registry.execute_setting("colors", Some("dark".to_string()), &mut settings);
    assert!(matches!(result, crate::command_line::executor::ExecutionResult::Success));
    assert_eq!(settings.theme, Some("dark".to_string()));
}

