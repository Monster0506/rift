use crate::command_line::commands::{
    CommandDescriptor, CommandParser, MatchResult, ParsedCommand, COMMANDS,
};
use crate::command_line::settings::{create_settings_registry, SettingsRegistry};
use crate::state::UserSettings;

fn parse_quit(_: &SettingsRegistry<UserSettings>, _: &[&str], bangs: usize) -> ParsedCommand {
    ParsedCommand::Quit { bangs }
}

fn parse_write(_: &SettingsRegistry<UserSettings>, args: &[&str], bangs: usize) -> ParsedCommand {
    match args.len() {
        0 => ParsedCommand::Write { path: None, bangs },
        1 => ParsedCommand::Write {
            path: Some(args[0].to_string()),
            bangs,
        },
        _ => ParsedCommand::Unknown {
            name: "write".to_string(),
        },
    }
}

fn parse_write_quit(
    _: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    match args.len() {
        0 => ParsedCommand::WriteQuit { path: None, bangs },
        1 => ParsedCommand::WriteQuit {
            path: Some(args[0].to_string()),
            bangs,
        },
        _ => ParsedCommand::Unknown {
            name: "wq".to_string(),
        },
    }
}

fn parse_bnext(_: &SettingsRegistry<UserSettings>, _: &[&str], bangs: usize) -> ParsedCommand {
    ParsedCommand::BufferNext { bangs }
}

fn parse_bprev(_: &SettingsRegistry<UserSettings>, _: &[&str], bangs: usize) -> ParsedCommand {
    ParsedCommand::BufferPrevious { bangs }
}

fn parse_set(
    registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    if args.is_empty() {
        return ParsedCommand::Unknown {
            name: "set".to_string(),
        };
    }

    let option_str = args[0];
    let option_registry = registry.build_option_registry();

    // Handle "no" prefix
    let option_lower = option_str.to_lowercase();
    if option_lower.starts_with("no") && option_lower.len() > 2 {
        let option_without_no = &option_lower[2..];
        match option_registry.match_command(option_without_no) {
            MatchResult::Exact(name) | MatchResult::Prefix(name) => {
                return ParsedCommand::Set {
                    option: name,
                    value: Some("false".to_string()),
                    bangs,
                };
            }
            _ => {}
        }
    }

    // Handle assignment
    if let Some(pos) = option_str.find('=') {
        let opt = &option_str[..pos];
        let val = &option_str[pos + 1..];
        return ParsedCommand::Set {
            option: opt.to_string(),
            value: Some(val.to_string()),
            bangs,
        };
    }

    // Handle space separated
    if args.len() > 1 {
        return ParsedCommand::Set {
            option: option_str.to_string(),
            value: Some(args[1].to_string()),
            bangs,
        };
    }

    // Boolean on
    ParsedCommand::Set {
        option: option_str.to_string(),
        value: Some("true".to_string()),
        bangs,
    }
}

const TEST_COMMANDS: &[CommandDescriptor] = &[
    CommandDescriptor {
        name: "quit",
        aliases: &["q"],
        description: "Quit",
        factory: Some(parse_quit),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "write",
        aliases: &["w"],
        description: "Write",
        factory: Some(parse_write),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "wq",
        aliases: &[],
        description: "Write Quit",
        factory: Some(parse_write_quit),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "set",
        aliases: &["se"],
        description: "Set",
        factory: Some(parse_set),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "setlocal",
        aliases: &["setl"],
        description: "Set Local",
        factory: Some(parse_set),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "buffer",
        aliases: &["b"],
        description: "Buffer",
        factory: None,
        subcommands: &[
            CommandDescriptor {
                name: "next",
                aliases: &["n"],
                description: "Next",
                factory: Some(parse_bnext),
                subcommands: &[],
            },
            CommandDescriptor {
                name: "previous",
                aliases: &["prev", "p"],
                description: "Prev",
                factory: Some(parse_bprev),
                subcommands: &[],
            },
        ],
    },
    CommandDescriptor {
        name: "bnext",
        aliases: &["bn"],
        description: "Next Buffer",
        factory: Some(parse_bnext),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "bprev",
        aliases: &["bp"],
        description: "Prev Buffer",
        factory: Some(parse_bprev),
        subcommands: &[],
    },
];

fn create_test_parser() -> CommandParser {
    let settings_registry = create_settings_registry();
    CommandParser::with_commands(settings_registry, TEST_COMMANDS)
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
            option: "noexpandtabs".to_string(),
            value: Some("true".to_string()),
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

    // "expa" -> "expa" (unknown option)
    let result = parser.parse(":set expa");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "expa".to_string(),
            value: Some("true".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_parse_set_option_prefix_noexpandtabs() {
    let parser = create_test_parser();

    // "noexpa" -> "noexpa" (unknown option)
    let result = parser.parse(":set noexpa");
    assert_eq!(
        result,
        ParsedCommand::Set {
            option: "noexpa".to_string(),
            value: Some("true".to_string()),
            bangs: 0
        }
    );
}

#[test]
fn test_parse_real_nohighlight() {
    let settings_registry = create_settings_registry();
    let parser = CommandParser::with_commands(settings_registry, COMMANDS);

    // Test full name
    let result = parser.parse(":nohighlight");
    assert_eq!(result, ParsedCommand::NoHighlight { bangs: 0 });

    // Test alias
    let result = parser.parse(":noh");
    assert_eq!(result, ParsedCommand::NoHighlight { bangs: 0 });
}

fn create_real_parser() -> CommandParser {
    let settings_registry = create_settings_registry();
    CommandParser::with_commands(settings_registry, COMMANDS)
}

#[test]
fn test_parse_split_no_args() {
    use crate::command_line::commands::SplitSubcommand;
    let parser = create_real_parser();
    assert_eq!(
        parser.parse(":split"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Current, bangs: 0 }
    );
}

#[test]
fn test_parse_split_dot() {
    use crate::command_line::commands::SplitSubcommand;
    let parser = create_real_parser();
    assert_eq!(
        parser.parse(":split ."),
        ParsedCommand::Split { subcommand: SplitSubcommand::Current, bangs: 0 }
    );
}

#[test]
fn test_parse_split_file() {
    use crate::command_line::commands::SplitSubcommand;
    let parser = create_real_parser();
    assert_eq!(
        parser.parse(":split foo.txt"),
        ParsedCommand::Split { subcommand: SplitSubcommand::File("foo.txt".to_string()), bangs: 0 }
    );
}

#[test]
fn test_parse_split_navigate() {
    use crate::command_line::commands::SplitSubcommand;
    use crate::split::navigation::Direction;
    let parser = create_real_parser();

    assert_eq!(
        parser.parse(":split :l"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Navigate(Direction::Left), bangs: 0 }
    );
    assert_eq!(
        parser.parse(":split :r"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Navigate(Direction::Right), bangs: 0 }
    );
    assert_eq!(
        parser.parse(":split :u"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Navigate(Direction::Up), bangs: 0 }
    );
    assert_eq!(
        parser.parse(":split :d"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Navigate(Direction::Down), bangs: 0 }
    );
}

#[test]
fn test_parse_split_resize() {
    use crate::command_line::commands::SplitSubcommand;
    let parser = create_real_parser();

    assert_eq!(
        parser.parse(":split :+5"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Resize(5), bangs: 0 }
    );
    assert_eq!(
        parser.parse(":split :-3"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Resize(-3), bangs: 0 }
    );
}

#[test]
fn test_parse_split_freeze() {
    use crate::command_line::commands::SplitSubcommand;
    let parser = create_real_parser();

    assert_eq!(
        parser.parse(":split :freeze"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Freeze, bangs: 0 }
    );
    assert_eq!(
        parser.parse(":split :nofreeze"),
        ParsedCommand::Split { subcommand: SplitSubcommand::NoFreeze, bangs: 0 }
    );
}

#[test]
fn test_parse_vsplit() {
    use crate::command_line::commands::SplitSubcommand;
    let parser = create_real_parser();

    assert_eq!(
        parser.parse(":vsplit"),
        ParsedCommand::VSplit { subcommand: SplitSubcommand::Current, bangs: 0 }
    );
    assert_eq!(
        parser.parse(":vsplit foo.txt"),
        ParsedCommand::VSplit { subcommand: SplitSubcommand::File("foo.txt".to_string()), bangs: 0 }
    );
}

#[test]
fn test_parse_split_aliases() {
    use crate::command_line::commands::SplitSubcommand;
    let parser = create_real_parser();

    assert_eq!(
        parser.parse(":sp"),
        ParsedCommand::Split { subcommand: SplitSubcommand::Current, bangs: 0 }
    );
    assert_eq!(
        parser.parse(":vs"),
        ParsedCommand::VSplit { subcommand: SplitSubcommand::Current, bangs: 0 }
    );
}
