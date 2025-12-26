use super::*;
use crate::command_line::registry::CommandDef;
use crate::command_line::settings::create_settings_registry;

fn create_test_registry() -> CommandRegistry {
    CommandRegistry::new()
        .register(CommandDef::new("quit").with_alias("q"))
        .register(CommandDef::new("set").with_alias("se"))
        .register(CommandDef::new("settings").with_alias("set"))
        .register(CommandDef::new("setup"))
        .register(CommandDef::new("write").with_alias("w"))
        .register(CommandDef::new("wq"))
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
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });

    let result = parser.parse("quit");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });
}

#[test]
fn test_parse_quit_alias() {
    let parser = create_test_parser();

    let result = parser.parse(":q");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });

    let result = parser.parse("q");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });
}

#[test]
fn test_parse_quit_prefix() {
    let parser = create_test_parser();

    let result = parser.parse(":qui");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });

    let result = parser.parse("qui");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });
}

#[test]
fn test_parse_quit_bangs() {
    let parser = create_test_parser();

    assert_eq!(parser.parse(":quit!"), ParsedCommand::Quit { bangs: 1 });
    assert_eq!(parser.parse(":q!"), ParsedCommand::Quit { bangs: 1 });
    assert_eq!(parser.parse(":q!!"), ParsedCommand::Quit { bangs: 2 });
}

#[test]
fn test_parse_write() {
    let parser = create_test_parser();

    assert_eq!(
        parser.parse(":write"),
        ParsedCommand::Write {
            path: None,
            bangs: 0
        }
    );
    assert_eq!(
        parser.parse(":w"),
        ParsedCommand::Write {
            path: None,
            bangs: 0
        }
    );
    assert_eq!(
        parser.parse(":w!"),
        ParsedCommand::Write {
            path: None,
            bangs: 1
        }
    );
    assert_eq!(
        parser.parse(":write filename.txt"),
        ParsedCommand::Write {
            path: Some("filename.txt".to_string()),
            bangs: 0
        }
    );
    assert_eq!(
        parser.parse(":w filename.txt"),
        ParsedCommand::Write {
            path: Some("filename.txt".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_parse_write_quit() {
    let parser = create_test_parser();

    assert_eq!(
        parser.parse(":wq"),
        ParsedCommand::WriteQuit {
            path: None,
            bangs: 0
        }
    );
    assert_eq!(
        parser.parse(":wq!"),
        ParsedCommand::WriteQuit {
            path: None,
            bangs: 1
        }
    );
    assert_eq!(
        parser.parse(":wq filename.txt"),
        ParsedCommand::WriteQuit {
            path: Some("filename.txt".to_string()),
            bangs: 0
        }
    );
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
            bangs: 0
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
            bangs: 0
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
            bangs: 0
        }
    );

    let result = parser.parse(":set tabwidth=8");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("8".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );

    let result = parser.parse(":set tabwidth 8");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("8".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );
}

#[test]
fn test_parse_set_prefix() {
    let parser = create_test_parser();

    // Test that "se" (alias for "set") correctly parses a set command
    let result = parser.parse(":se expandtabs");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
            bangs: 0
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
    // Create a registry where "se" is ambiguous between "setup" and "settings"
    let registry = CommandRegistry::new()
        .register(CommandDef::new("setup"))
        .register(CommandDef::new("settings"));
    let settings_registry = create_settings_registry();
    let parser = CommandParser::new(registry, settings_registry);

    let result = parser.parse(":se");
    assert!(matches!(result, ParsedCommand::Ambiguous { .. }));
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
            bangs: 0
        }
    );

    let result = parser.parse(":set tabwidth   4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_parse_set_value_with_spaces() {
    let parser = create_test_parser();

    // Assignment syntax only takes the value up to the first space
    let result = parser.parse(":set option=value with spaces");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "option".to_string(),
            value: Some("value".to_string()),
            bangs: 0
        }
    );
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
            bangs: 0
        }
    );

    // Option names are normalized to canonical lowercase via registry matching
    let result = parser.parse(":Set EXPANDTABS");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );
}

#[test]
fn test_parse_whitespace_handling() {
    let parser = create_test_parser();

    let result = parser.parse("  :quit  ");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });

    let result = parser.parse("  quit  ");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });
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
            bangs: 0
        }
    );

    // "noexpandtabs" should be parsed as "expandtabs" with value "false"
    let result = parser.parse(":set noexpandtabs");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );

    // Space-separated
    let result = parser.parse(":set option value");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "option".to_string(),
            value: Some("value".to_string()),
            bangs: 0
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
            bangs: 0
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
    // Test that explicit aliases take precedence over prefix matching
    // "q" should match "quit" via explicit alias, not be ambiguous with "query"
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit").with_alias("q"))
        .register(CommandDef::new("query"));
    let settings_registry = create_settings_registry();
    let parser = CommandParser::new(registry, settings_registry);

    let result = parser.parse(":q");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });
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
            bangs: 0
        }
    );

    // "exp" should match "expandtabs" (unambiguous)
    let result = parser.parse(":set exp");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
            bangs: 0
        }
    );

    // "et" should match "expandtabs" via alias
    let result = parser.parse(":set et");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("true".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );

    // "noexp" should match "noexpandtabs"
    let result = parser.parse(":set noexp");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
            bangs: 0
        }
    );

    // "noet" should match "noexpandtabs" via alias
    let result = parser.parse(":set noet");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );

    // "tab" should match "tabwidth" (unambiguous)
    let result = parser.parse(":set tab 8");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("8".to_string()),
            bangs: 0
        }
    );

    // "tw" should match "tabwidth" via alias
    let result = parser.parse(":set tw=16");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("16".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );

    let result = parser.parse(":set tabw=4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );

    let result = parser.parse(":set tabw 4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
            bangs: 0
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
            bangs: 0
        }
    );

    let result = parser.parse(":set NOEXPA");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
            bangs: 0
        }
    );

    let result = parser.parse(":set TABW=4");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some("4".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_parse_write_no_args() {
    let parser = create_test_parser();

    let result = parser.parse(":w");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: None,
            bangs: 0
        }
    );

    let result = parser.parse(":write");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: None,
            bangs: 0
        }
    );
}

#[test]
fn test_parse_write_with_filename() {
    let parser = create_test_parser();

    let result = parser.parse(":w file.txt");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: Some("file.txt".to_string()),
            bangs: 0
        }
    );

    let result = parser.parse(":write file.txt");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: Some("file.txt".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_parse_write_alias() {
    let parser = create_test_parser();

    let result = parser.parse(":w");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: None,
            bangs: 0
        }
    );
}

#[test]
fn test_parse_write_too_many_args() {
    let parser = create_test_parser();

    let result = parser.parse(":w file1 file2");
    assert!(matches!(
        result,
        ParsedCommand::Unknown { name } if name.contains("too many arguments")
    ));

    let result = parser.parse(":write file1 file2 file3");
    assert!(matches!(
        result,
        ParsedCommand::Unknown { name } if name.contains("too many arguments")
    ));
}

#[test]
fn test_parse_write_quit_no_args() {
    let parser = create_test_parser();

    let result = parser.parse(":wq");
    assert_eq!(
        result,
        ParsedCommand::WriteQuit {
            path: None,
            bangs: 0
        }
    );
}

#[test]
fn test_parse_write_quit_with_filename() {
    let parser = create_test_parser();

    let result = parser.parse(":wq file.txt");
    assert_eq!(
        result,
        ParsedCommand::WriteQuit {
            path: Some("file.txt".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_parse_write_quit_too_many_args() {
    let parser = create_test_parser();

    let result = parser.parse(":wq file1 file2");
    assert!(matches!(
        result,
        ParsedCommand::Unknown { name } if name.contains("too many arguments")
    ));
}

#[test]
fn test_parse_write_prefix() {
    let parser = create_test_parser();

    // "wr" should match "write"
    let result = parser.parse(":wr");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: None,
            bangs: 0
        }
    );

    let result = parser.parse(":wri file.txt");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: Some("file.txt".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_strip_bangs() {
    // Basic cases
    assert_eq!(CommandParser::strip_bangs("quit"), ("quit", 0));
    assert_eq!(CommandParser::strip_bangs("quit!"), ("quit", 1));
    assert_eq!(CommandParser::strip_bangs("quit!!"), ("quit", 2));

    // Edge cases
    assert_eq!(CommandParser::strip_bangs("!"), ("", 1));
    assert_eq!(CommandParser::strip_bangs("!!"), ("", 2));
    assert_eq!(CommandParser::strip_bangs(""), ("", 0));

    // Intermixed (should only strip trailing)
    assert_eq!(CommandParser::strip_bangs("qu!it"), ("qu!it", 0));
    assert_eq!(CommandParser::strip_bangs("qu!it!"), ("qu!it", 1));
}
