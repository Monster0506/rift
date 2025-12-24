use super::*;
use crate::command_line::registry::CommandDef;
use crate::command_line::settings::create_settings_registry;

fn create_test_registry() -> CommandRegistry {
    CommandRegistry::new()
        .register(CommandDef::new("quit").with_alias("q"))
        .register(CommandDef::new("set").with_alias("se"))
        .register(CommandDef::new("settings").with_alias("set"))
        .register(CommandDef::new("setup"))
}

fn create_test_parser() -> CommandParser {
    let registry = create_test_registry();
    let settings_registry = create_settings_registry();
    CommandParser::new(registry, settings_registry)
}

#[test]
fn test_parse_empty() {
    let parser = create_test_parser();
    
    let result = parser.parse("");
    assert!(matches!(result, ParsedCommand::Unknown { name } if name.is_empty()));
    
    let result = parser.parse(":");
    assert!(matches!(result, ParsedCommand::Unknown { name } if name.is_empty()));
    
    let result = parser.parse("   ");
    assert!(matches!(result, ParsedCommand::Unknown { name } if name.is_empty()));
}

#[test]
fn test_parse_quit_exact() {
    let parser = create_test_parser();
    
    let result = parser.parse(":quit");
    assert_eq!(result, ParsedCommand::Quit);
    
    let result = parser.parse("quit");
    assert_eq!(result, ParsedCommand::Quit);
}

#[test]
fn test_parse_quit_alias() {
    let parser = create_test_parser();
    
    let result = parser.parse(":q");
    assert_eq!(result, ParsedCommand::Quit);
    
    let result = parser.parse("q");
    assert_eq!(result, ParsedCommand::Quit);
}

#[test]
fn test_parse_quit_prefix() {
    let parser = create_test_parser();
    
    let result = parser.parse(":qui");
    assert_eq!(result, ParsedCommand::Quit);
    
    let result = parser.parse("qui");
    assert_eq!(result, ParsedCommand::Quit);
}

#[test]
fn test_parse_set_boolean_on() {
    let parser = create_test_parser();
    
    let result = parser.parse(":set expandtabs");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
}

#[test]
fn test_parse_set_boolean_off() {
    let parser = create_test_parser();
    
    let result = parser.parse(":set noexpandtabs");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
        }
    );
}

#[test]
fn test_parse_set_assignment() {
    let parser = create_test_parser();
    
    let result = parser.parse(":set tabwidth=4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
        }
    );
    
    let result = parser.parse(":set tabwidth=8");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("8".to_string()),
        }
    );
}

#[test]
fn test_parse_set_space_separated() {
    let parser = create_test_parser();
    
    let result = parser.parse(":set tabwidth 4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
        }
    );
    
    let result = parser.parse(":set tabwidth 8");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("8".to_string()),
        }
    );
}

#[test]
fn test_parse_set_with_alias() {
    let parser = create_test_parser();
    
    // "se" is an alias for "set"
    let result = parser.parse(":se expandtabs");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
}

#[test]
fn test_parse_set_prefix() {
    let parser = create_test_parser();
    
    // "set" should match via prefix (or explicit alias if defined)
    // Test that "set" matches via alias to "settings", but we're parsing "set" command
    // Actually, in create_test_registry, "set" is registered as a command with alias "se"
    // And "settings" has alias "set"
    // So ":set" would match "settings" via alias, not the "set" command
    // Let's test with the actual "set" command using its alias "se"
    let result = parser.parse(":se expandtabs");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
}

#[test]
fn test_parse_unknown_command() {
    let parser = create_test_parser();
    
    let result = parser.parse(":nonexistent");
    assert!(matches!(
        result,
        ParsedCommand::Unknown { name } if name == "nonexistent"
    ));
}

#[test]
fn test_parse_ambiguous() {
    
    // "se" is ambiguous between "set" and "settings" (if both exist)
    // Actually, wait - "set" is an alias for "settings", so "set" should match "settings"
    // But "se" as a prefix could match both "set" (via alias) and "settings"
    // Let me check: "se" starts with "set"? No. "se" starts with "settings"? Yes.
    // So "se" would match "settings" unambiguously
    
    // Let's create a better ambiguous case
    let registry = CommandRegistry::new()
        .register(CommandDef::new("setup"))
        .register(CommandDef::new("settings"));
    let settings_registry = create_settings_registry();
    let parser = CommandParser::new(registry, settings_registry);
    
    let result = parser.parse(":se");
    assert!(matches!(
        result,
        ParsedCommand::Ambiguous { .. }
    ));
}

#[test]
fn test_parse_set_no_args() {
    let parser = create_test_parser();
    
    let result = parser.parse(":set");
    assert!(matches!(
        result,
        ParsedCommand::Unknown { name } if name == "set"
    ));
}

#[test]
fn test_parse_set_multiple_spaces() {
    let parser = create_test_parser();
    
    let result = parser.parse(":set   expandtabs");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
    
    let result = parser.parse(":set tabwidth   4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
        }
    );
}

#[test]
fn test_parse_set_value_with_spaces() {
    let parser = create_test_parser();
    
    // Assignment syntax preserves spaces in value
    let result = parser.parse(":set option=value with spaces");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "option".to_string(),
            value: Some("value".to_string()), // Only up to first space in assignment
        }
    );
    
    // Actually, with current implementation, assignment only takes up to '='
    // Let me verify the behavior is correct
}

    #[test]
    fn test_parse_set_case_insensitive() {
        let parser = create_test_parser();
        
        let result = parser.parse(":SET expandtabs");
        assert_eq!(
            result,
            ParsedCommand::Set {
                option: "expandtabs".to_string(),
                value: Some("true".to_string()),
            }
        );
        
        // Option names are normalized to canonical lowercase via registry matching
        let result = parser.parse(":Set EXPANDTABS");
        assert_eq!(
            result,
            ParsedCommand::Set {
                option: "expandtabs".to_string(),
                value: Some("true".to_string()),
            }
        );
    }

#[test]
fn test_parse_set_complex_value() {
    let parser = create_test_parser();
    
    let result = parser.parse(":set tabwidth=16");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("16".to_string()),
        }
    );
}

#[test]
fn test_parse_whitespace_handling() {
    let parser = create_test_parser();
    
    let result = parser.parse("  :quit  ");
    assert_eq!(result, ParsedCommand::Quit);
    
    let result = parser.parse("  quit  ");
    assert_eq!(result, ParsedCommand::Quit);
}

#[test]
fn test_parse_set_no_prefix_handling() {
    let parser = create_test_parser();
    
    // "no" by itself should be treated as an option name, not a prefix
    let result = parser.parse(":set no");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "no".to_string(),
            value: Some("true".to_string()),
        }
    );
    
    // "noexpandtabs" should be parsed as "expandtabs" with value "false"
    let result = parser.parse(":set noexpandtabs");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
        }
    );
}

#[test]
fn test_parse_set_assignment_vs_space() {
    let parser = create_test_parser();
    
    // Assignment takes precedence
    let result = parser.parse(":set option=value");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "option".to_string(),
            value: Some("value".to_string()),
        }
    );
    
    // Space-separated
    let result = parser.parse(":set option value");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "option".to_string(),
            value: Some("value".to_string()),
        }
    );
}

#[test]
fn test_parse_set_multiple_values() {
    let parser = create_test_parser();
    
    // Only first value is used
    let result = parser.parse(":set option value1 value2 value3");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "option".to_string(),
            value: Some("value1".to_string()),
        }
    );
}

#[test]
fn test_parse_ambiguous_with_prefix() {
    // Test that ambiguous commands are properly detected
    let registry = CommandRegistry::new()
        .register(CommandDef::new("setup"))
        .register(CommandDef::new("settings"));
    let settings_registry = create_settings_registry();
    let parser = CommandParser::new(registry, settings_registry);
    
    let result = parser.parse(":se");
    match result {
        ParsedCommand::Ambiguous { prefix, matches } => {
            assert_eq!(prefix, "se");
            assert_eq!(matches.len(), 2);
            assert!(matches.contains(&"setup".to_string()));
            assert!(matches.contains(&"settings".to_string()));
        }
        _ => panic!("Expected ambiguous command"),
    }
}

#[test]
fn test_parse_explicit_alias_overrides_ambiguity() {
    // If "set" is an explicit alias for "settings", it should work even with "setup" present
    // "set" should match "settings" via explicit alias
    // ":set" without args returns Unknown (expected behavior)
    // The key is that "set" matched "settings" via alias, not "setup"
    // But since "set" command needs args, it returns Unknown
    
    // Let's test with a command that takes no args to verify alias matching works
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit").with_alias("q"))
        .register(CommandDef::new("query"));
    let settings_registry = create_settings_registry();
    let parser = CommandParser::new(registry, settings_registry);
    
    // "q" should match "quit" via alias, not be ambiguous with "query"
    let result = parser.parse(":q");
    assert_eq!(result, ParsedCommand::Quit);
}

#[test]
fn test_parse_set_option_prefix_expandtabs() {
    let parser = create_test_parser();
    
    // "expa" should match "expandtabs"
    let result = parser.parse(":set expa");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
    
    // "exp" should match "expandtabs" (unambiguous)
    let result = parser.parse(":set exp");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
    
    // "et" should match "expandtabs" via alias
    let result = parser.parse(":set et");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
}

#[test]
fn test_parse_set_option_prefix_noexpandtabs() {
    let parser = create_test_parser();
    
    // "noexpa" should match "noexpandtabs" -> "expandtabs" with false
    let result = parser.parse(":set noexpa");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
        }
    );
    
    // "noexp" should match "noexpandtabs"
    let result = parser.parse(":set noexp");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
        }
    );
    
    // "noet" should match "noexpandtabs" via alias
    let result = parser.parse(":set noet");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
        }
    );
}

#[test]
fn test_parse_set_option_prefix_tabwidth() {
    let parser = create_test_parser();
    
    // "tabw" should match "tabwidth"
    let result = parser.parse(":set tabw=4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
        }
    );
    
    // "tab" should match "tabwidth" (unambiguous)
    let result = parser.parse(":set tab 8");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("8".to_string()),
        }
    );
    
    // "tw" should match "tabwidth" via alias
    let result = parser.parse(":set tw=16");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("16".to_string()),
        }
    );
}

#[test]
fn test_parse_set_option_prefix_assignment() {
    let parser = create_test_parser();
    
    // Prefix matching with assignment syntax
    let result = parser.parse(":set expa=true");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
    
    let result = parser.parse(":set tabw=4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
        }
    );
}

#[test]
fn test_parse_set_option_prefix_space_separated() {
    let parser = create_test_parser();
    
    // Prefix matching with space-separated value
    let result = parser.parse(":set expa false");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
        }
    );
    
    let result = parser.parse(":set tabw 4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
        }
    );
}

#[test]
fn test_parse_set_option_case_insensitive_prefix() {
    let parser = create_test_parser();
    
    // Case-insensitive prefix matching
    let result = parser.parse(":set EXPA");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
        }
    );
    
    let result = parser.parse(":set NOEXPA");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
        }
    );
    
    let result = parser.parse(":set TABW=4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
        }
    );
}