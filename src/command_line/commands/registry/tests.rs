use crate::command_line::commands::{CommandDef, CommandRegistry, MatchResult};

#[test]
fn test_empty_registry() {
    let registry = CommandRegistry::new();
    assert!(matches!(
        registry.match_command("test"),
        MatchResult::Unknown(_)
    ));
}

#[test]
fn test_exact_match_command_name() {
    let registry = CommandRegistry::new().register(CommandDef::new("quit"));

    match registry.match_command("quit") {
        MatchResult::Exact(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected exact match"),
    }
}

#[test]
fn test_exact_match_case_insensitive() {
    let registry = CommandRegistry::new().register(CommandDef::new("quit"));

    match registry.match_command("QUIT") {
        MatchResult::Exact(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected exact match"),
    }

    match registry.match_command("Quit") {
        MatchResult::Exact(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected exact match"),
    }
}

#[test]
fn test_exact_match_explicit_alias() {
    let registry = CommandRegistry::new().register(CommandDef::new("quit").with_alias("q"));

    match registry.match_command("q") {
        MatchResult::Exact(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected exact match via alias"),
    }
}

#[test]
fn test_explicit_alias_overrides_prefix() {
    // If "set" is an explicit alias for "settings", it should match even if "setup" exists
    let registry = CommandRegistry::new()
        .register(CommandDef::new("settings").with_alias("set"))
        .register(CommandDef::new("setup"));

    match registry.match_command("set") {
        MatchResult::Exact(name) => assert_eq!(name, "settings"),
        _ => panic!("Expected exact match via explicit alias, not ambiguous"),
    }
}

#[test]
fn test_shortest_unambiguous_prefix() {
    let registry = CommandRegistry::new()
        .register(CommandDef::new("setup"))
        .register(CommandDef::new("settings"));

    // "setu" should match "setup" (shortest unambiguous)
    match registry.match_command("setu") {
        MatchResult::Prefix(name) => assert_eq!(name, "setup"),
        _ => panic!("Expected prefix match for 'setu'"),
    }

    // "sett" should match "settings" (shortest unambiguous)
    match registry.match_command("sett") {
        MatchResult::Prefix(name) => assert_eq!(name, "settings"),
        _ => panic!("Expected prefix match for 'sett'"),
    }
}

#[test]
fn test_ambiguous_prefix() {
    let registry = CommandRegistry::new()
        .register(CommandDef::new("setup"))
        .register(CommandDef::new("settings"));

    // "se" matches both "setup" and "settings" - should be ambiguous
    match registry.match_command("se") {
        MatchResult::Ambiguous { prefix, matches } => {
            assert_eq!(prefix, "se");
            assert_eq!(matches.len(), 2);
            assert!(matches.contains(&"setup".to_string()));
            assert!(matches.contains(&"settings".to_string()));
        }
        _ => panic!("Expected ambiguous match"),
    }
}

#[test]
fn test_prefix_match_single_char() {
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit").with_alias("q"))
        .register(CommandDef::new("query"));

    // "q" should match "quit" via explicit alias, not "query"
    match registry.match_command("q") {
        MatchResult::Exact(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected exact match via alias"),
    }

    // "qu" should be ambiguous (matches both "quit" and "query")
    match registry.match_command("qu") {
        MatchResult::Ambiguous { matches, .. } => {
            assert_eq!(matches.len(), 2);
            assert!(matches.contains(&"quit".to_string()));
            assert!(matches.contains(&"query".to_string()));
        }
        _ => panic!("Expected ambiguous match for 'qu'"),
    }

    // "qui" should match "quit" (unambiguous prefix)
    match registry.match_command("qui") {
        MatchResult::Prefix(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected prefix match for 'qui'"),
    }

    // "quer" should match "query" (unambiguous prefix)
    match registry.match_command("quer") {
        MatchResult::Prefix(name) => assert_eq!(name, "query"),
        _ => panic!("Expected prefix match for 'quer'"),
    }
}

#[test]
fn test_multiple_aliases() {
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit").with_aliases(vec!["q", "exit", "bye"]));

    for alias in &["q", "exit", "bye"] {
        match registry.match_command(alias) {
            MatchResult::Exact(name) => assert_eq!(name, "quit"),
            _ => panic!("Expected exact match for alias '{}'", alias),
        }
    }
}

#[test]
fn test_prefix_with_aliases() {
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit").with_alias("q"))
        .register(CommandDef::new("query"));

    // "qui" should match "quit" (prefix of command name)
    match registry.match_command("qui") {
        MatchResult::Prefix(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected prefix match"),
    }

    // "quer" should match "query" (prefix of command name)
    match registry.match_command("quer") {
        MatchResult::Prefix(name) => assert_eq!(name, "query"),
        _ => panic!("Expected prefix match"),
    }
}

#[test]
fn test_unknown_command() {
    let registry = CommandRegistry::new().register(CommandDef::new("quit"));

    match registry.match_command("nonexistent") {
        MatchResult::Unknown(input) => assert_eq!(input, "nonexistent"),
        _ => panic!("Expected unknown command"),
    }
}

#[test]
fn test_empty_input() {
    let registry = CommandRegistry::new().register(CommandDef::new("quit"));

    match registry.match_command("") {
        MatchResult::Unknown(input) => assert_eq!(input, ""),
        _ => panic!("Expected unknown for empty input"),
    }

    match registry.match_command("   ") {
        MatchResult::Unknown(input) => assert_eq!(input, ""),
        _ => panic!("Expected unknown for whitespace-only input"),
    }
}

#[test]
fn test_whitespace_trimming() {
    let registry = CommandRegistry::new().register(CommandDef::new("quit"));

    match registry.match_command("  quit  ") {
        MatchResult::Exact(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected exact match after trimming"),
    }
}

#[test]
fn test_complex_scenario() {
    // Real-world scenario: setup, settings, set (alias for settings)
    let registry = CommandRegistry::new()
        .register(CommandDef::new("settings").with_alias("set"))
        .register(CommandDef::new("setup"));

    // "set" should match "settings" via explicit alias
    match registry.match_command("set") {
        MatchResult::Exact(name) => assert_eq!(name, "settings"),
        _ => panic!("Expected exact match via alias"),
    }

    // "setu" should match "setup" (unambiguous prefix)
    match registry.match_command("setu") {
        MatchResult::Prefix(name) => assert_eq!(name, "setup"),
        _ => panic!("Expected prefix match"),
    }

    // "sett" should match "settings" (unambiguous prefix)
    match registry.match_command("sett") {
        MatchResult::Prefix(name) => assert_eq!(name, "settings"),
        _ => panic!("Expected prefix match"),
    }

    // "se" should be ambiguous
    match registry.match_command("se") {
        MatchResult::Ambiguous { matches, .. } => {
            assert_eq!(matches.len(), 2);
        }
        _ => panic!("Expected ambiguous match"),
    }
}

#[test]
fn test_alias_prefix_matching() {
    // Test that aliases can also be matched by prefix
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit").with_alias("q"))
        .register(CommandDef::new("query"));

    // "qu" matches both "quit" (command name starts with "qu") and "query" (command name starts with "qu")
    // So it should be ambiguous
    match registry.match_command("qu") {
        MatchResult::Ambiguous { matches, .. } => {
            assert_eq!(matches.len(), 2);
            assert!(matches.contains(&"quit".to_string()));
            assert!(matches.contains(&"query".to_string()));
        }
        _ => panic!("Expected ambiguous match for 'qu'"),
    }

    // But "q" matches "quit" exactly via alias
    match registry.match_command("q") {
        MatchResult::Exact(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected exact match via alias"),
    }
}

#[test]
fn test_command_names() {
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit"))
        .register(CommandDef::new("write"));

    let names = registry.command_names();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&&"quit".to_string()));
    assert!(names.contains(&&"write".to_string()));
}

#[test]
fn test_single_char_prefix_unambiguous() {
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit"))
        .register(CommandDef::new("write"));

    // "q" should match "quit" (unambiguous)
    match registry.match_command("q") {
        MatchResult::Prefix(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected prefix match"),
    }

    // "w" should match "write" (unambiguous)
    match registry.match_command("w") {
        MatchResult::Prefix(name) => assert_eq!(name, "write"),
        _ => panic!("Expected prefix match"),
    }
}

#[test]
fn test_single_char_prefix_ambiguous() {
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit"))
        .register(CommandDef::new("query"));

    // "q" matches both "quit" and "query" - ambiguous
    match registry.match_command("q") {
        MatchResult::Ambiguous { matches, .. } => {
            assert_eq!(matches.len(), 2);
            assert!(matches.contains(&"quit".to_string()));
            assert!(matches.contains(&"query".to_string()));
        }
        _ => panic!("Expected ambiguous match"),
    }
}

#[test]
fn test_exact_alias_vs_prefix() {
    // If "q" is an alias for "quit", and we also have "query",
    // then "q" should match "quit" exactly, not be ambiguous
    let registry = CommandRegistry::new()
        .register(CommandDef::new("quit").with_alias("q"))
        .register(CommandDef::new("query"));

    match registry.match_command("q") {
        MatchResult::Exact(name) => assert_eq!(name, "quit"),
        _ => panic!("Expected exact match via alias, not ambiguous"),
    }
}
