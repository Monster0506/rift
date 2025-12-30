use crate::command_line::commands::{CommandParser, ParsedCommand};
use crate::command_line::settings::create_settings_registry;

fn create_test_parser() -> CommandParser {
    let settings_registry = create_settings_registry();
    CommandParser::new(settings_registry)
}

#[test]
fn test_parse_empty() {
    let parser = create_test_parser();

    assert!(matches!(parser.parse(""), ParsedCommand::Unknown { .. }));

    assert!(matches!(parser.parse(":"), ParsedCommand::Unknown { .. }));

    assert!(matches!(parser.parse("   "), ParsedCommand::Unknown { .. }));
}

#[test]
fn test_parse_quit_exact() {
    let parser = create_test_parser();
    let result = parser.parse(":quit");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });
}

#[test]
fn test_parse_quit_alias() {
    let parser = create_test_parser();
    let result = parser.parse(":q");
    assert_eq!(result, ParsedCommand::Quit { bangs: 0 });
}

#[test]
fn test_parse_quit_bangs() {
    let parser = create_test_parser();
    let result = parser.parse(":quit!");
    assert_eq!(result, ParsedCommand::Quit { bangs: 1 });

    let result = parser.parse(":q!");
    assert_eq!(result, ParsedCommand::Quit { bangs: 1 });
}

#[test]
fn test_parse_write() {
    let parser = create_test_parser();
    let result = parser.parse(":write");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: None,
            bangs: 0
        }
    );

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
fn test_parse_write_with_filename() {
    let parser = create_test_parser();
    let result = parser.parse(":write test.txt");
    assert_eq!(
        result,
        ParsedCommand::Write {
            path: Some("test.txt".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_parse_write_quit() {
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
fn test_parse_ambiguous() {
    let parser = create_test_parser();
    // "s" is ambiguous between "set" and "setlocal"
    let result = parser.parse(":s");
    assert!(matches!(result, ParsedCommand::Ambiguous { .. }));

    if let ParsedCommand::Ambiguous { prefix, matches } = result {
        assert_eq!(prefix, "s");
        assert!(matches.contains(&"set".to_string()));
        assert!(matches.contains(&"setlocal".to_string()));
    }
}

#[test]
fn test_parse_explicit_alias_overrides_ambiguity() {
    let parser = create_test_parser();
    // "b" is alias for "buffer", but also prefix for "bnext" and "bprev"
    // Should match "buffer" exactly via alias
    let result = parser.parse(":b");
    // buffer command has no factory, so it returns Unknown with name "buffer"
    // This confirms it matched "buffer" and not "bnext" or Ambiguous
    match result {
        ParsedCommand::Unknown { name } => assert_eq!(name, "buffer"),
        _ => panic!("Expected Unknown command 'buffer', got {:?}", result),
    }
}

#[test]
fn test_parse_subcommands() {
    let parser = create_test_parser();

    // Test buffer next
    match parser.parse(":buffer next") {
        ParsedCommand::BufferNext { bangs } => assert_eq!(bangs, 0),
        _ => panic!("Expected BufferNext command"),
    }

    // Test buffer prev
    match parser.parse(":buffer previous") {
        ParsedCommand::BufferPrevious { bangs } => assert_eq!(bangs, 0),
        _ => panic!("Expected BufferPrevious command"),
    }

    // Test buffer prev alias
    match parser.parse(":buffer prev") {
        ParsedCommand::BufferPrevious { bangs } => assert_eq!(bangs, 0),
        _ => panic!("Expected BufferPrevious command"),
    }

    // Test partial match (buffer only)
    match parser.parse(":buffer") {
        ParsedCommand::Unknown { name } => assert_eq!(name, "buffer"),
        _ => panic!("Expected Unknown command 'buffer'"),
    }

    // Test aliases
    match parser.parse(":bnext") {
        ParsedCommand::BufferNext { bangs } => assert_eq!(bangs, 0),
        _ => panic!("Expected BufferNext command from alias"),
    }

    match parser.parse(":bprev") {
        ParsedCommand::BufferPrevious { bangs } => assert_eq!(bangs, 0),
        _ => panic!("Expected BufferPrevious command from alias"),
    }
}

#[test]
fn test_parse_set_option_prefix_expandtabs() {
    let parser = create_test_parser();

    // "expa" -> "expandtabs"
    let result = parser.parse(":set expa");
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

    // "noexpa" -> "noexpandtabs" -> expandtabs = false
    let result = parser.parse(":set noexpa");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expandtabs".to_string(),
            value: Some("false".to_string()),
            bangs: 0
        }
    );
}
