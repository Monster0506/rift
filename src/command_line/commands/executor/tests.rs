//! Tests for command executor

use crate::command_line::commands::executor::{CommandExecutor, ExecutionResult};
use crate::command_line::commands::types::ParsedCommand;
use crate::document::Document;
use crate::state::definitions::create_settings_registry;
use crate::state::State;

#[test]
fn test_execute_quit() {
    let mut state = State::new();
    let command = ParsedCommand::Quit { bangs: 0 };

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let result = CommandExecutor::execute(command, &mut state, &mut document);
    assert_eq!(result, ExecutionResult::Quit { bangs: 0 });
}

#[test]
fn test_execute_unknown_command() {
    let mut state = State::new();

    let command = ParsedCommand::Unknown {
        name: "nonexistent".to_string(),
        args: vec!["arg1".to_string()],
    };

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let result = CommandExecutor::execute(command, &mut state, &mut document);
    // Unknown commands are passed to the plugin host as PluginCommand.
    // The editor checks the plugin host before reporting an error.
    match result {
        ExecutionResult::PluginCommand { name, args } => {
            assert_eq!(name, "nonexistent");
            assert_eq!(args, vec!["arg1".to_string()]);
        }
        _ => panic!("Expected PluginCommand for unknown command, got {result:?}"),
    }
}

#[test]
fn test_execute_ambiguous_command() {
    let mut state = State::new();

    let command = ParsedCommand::Ambiguous {
        prefix: "se".to_string(),
        matches: vec!["setup".to_string(), "settings".to_string()],
    };

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let result = CommandExecutor::execute(command, &mut state, &mut document);
    match result {
        ExecutionResult::Failure => {
            assert!(state
                .notifications
                .iter_active()
                .any(|n| n.message.contains("Ambiguous command")));
            assert!(state
                .notifications
                .iter_active()
                .any(|n| n.message.contains("se")));
            assert!(state
                .notifications
                .iter_active()
                .any(|n| n.message.contains("setup")));
            assert!(state
                .notifications
                .iter_active()
                .any(|n| n.message.contains("settings")));
        }
        _ => panic!("Expected error for ambiguous command"),
    }
}

#[test]
fn test_substitute_current_line() {
    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    document.replace_content("foo bar foo", crate::document::ops::ContentKind::Render);
    // Cursor at 0
    document
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();

    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: String::new(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(matches!(result, ExecutionResult::Substituted(_)));
    assert_eq!(document.buffer().to_string(), "baz bar foo");
}

#[test]
fn test_substitute_global_line() {
    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    document.replace_content("foo bar foo", crate::document::ops::ContentKind::Render);
    document
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();

    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: "g".to_string(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(matches!(result, ExecutionResult::Substituted(_)));
    assert_eq!(document.buffer().to_string(), "baz bar baz");
}

#[test]
fn test_substitute_whole_file() {
    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    document.replace_content("foo\nbar\nfoo", crate::document::ops::ContentKind::Render);
    document
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();

    // :s%/foo/baz/g
    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: "g".to_string(),
        range: Some("%".to_string()),
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(matches!(result, ExecutionResult::Substituted(_)));
    assert_eq!(document.buffer().to_string(), "baz\nbar\nbaz");
}

#[test]
fn test_substitute_case_insensitive() {
    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    document.replace_content("Foo bar foo", crate::document::ops::ContentKind::Render);
    document
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();

    // Smart case: lowercase pattern "foo" matches both "Foo" and "foo" (case-insensitive)
    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: String::new(), // subst flags
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(matches!(result, ExecutionResult::Substituted(_)));
    assert_eq!(document.buffer().to_string(), "baz bar foo"); // First match only (no 'g' flag)
}

#[test]
fn test_substitute_no_match() {
    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    document.replace_content("hello world", crate::document::ops::ContentKind::Render);

    let command = ParsedCommand::Substitute {
        pattern: "nothere".to_string(),
        replacement: "baz".to_string(),
        flags: String::new(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert_eq!(result, ExecutionResult::Failure);
    assert_eq!(document.buffer().revision(), 1); // Only insert incremented it
    assert!(state
        .notifications
        .iter_active()
        .any(|n| n.message.contains("Pattern not found")));
}

#[test]
fn test_substitute_parsing_requires_space() {
    let settings_registry = create_settings_registry();
    let command_parser =
        crate::command_line::commands::parser::CommandParser::new(settings_registry);

    // :s/foo/bar (no space) -> Unknown command "s/foo/bar"
    let input = "s/foo/bar";
    let command = command_parser.parse(input);

    match command {
        ParsedCommand::Unknown { name, .. } => {
            assert_eq!(name, "s/foo/bar");
        }
        _ => panic!("Expected Unknown command, got {command:?}"),
    }
}

#[test]
fn test_substitute_parsing_with_space() {
    let settings_registry = create_settings_registry();
    let command_parser =
        crate::command_line::commands::parser::CommandParser::new(settings_registry);

    // :s /foo/bar (with space) -> Valid Substitute command
    let input = "s /foo/bar";
    let command = command_parser.parse(input);

    match command {
        ParsedCommand::Substitute {
            pattern,
            replacement,
            flags,
            range,
            bangs,
        } => {
            assert_eq!(pattern, "foo");
            assert_eq!(replacement, "bar");
            assert_eq!(flags, "");
            assert_eq!(range, None);
            assert_eq!(bangs, 0);
        }
        _ => panic!("Expected Substitute command, got {command:?}"),
    }
}

#[test]
fn test_g_is_an_alias_for_the_git_command() {
    // Fugitive convention: `:G` is a synonym for `:Git`.
    let settings_registry = create_settings_registry();
    let command_parser =
        crate::command_line::commands::parser::CommandParser::new(settings_registry);

    match command_parser.parse("G log") {
        ParsedCommand::Git { args, .. } => assert_eq!(args, "log"),
        other => panic!("Expected Git command via the G alias, got {other:?}"),
    }

    match command_parser.parse("G") {
        ParsedCommand::Git { args, .. } => assert_eq!(args, ""),
        other => panic!("Expected bare Git command via the G alias, got {other:?}"),
    }
}

#[test]
fn test_substitute_parsing_weird_behavior_percent() {
    let settings_registry = create_settings_registry();
    let command_parser =
        crate::command_line::commands::parser::CommandParser::new(settings_registry);

    let input = "s % /ABC/XYZ";
    let command = command_parser.parse(input);

    match command {
        ParsedCommand::Substitute {
            pattern,
            replacement,
            flags,
            range,
            bangs,
        } => {
            assert_eq!(pattern, " /ABC/XYZ");
            assert_eq!(replacement, "");
            assert_eq!(flags, "");
            assert_eq!(range, None);
            assert_eq!(bangs, 0);
        }
        _ => panic!("Expected Substitute command, got {command:?}"),
    }
}

#[test]
fn test_substitute_undo_redo() {
    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();

    // Initial content
    document.replace_content("foo bar foo", crate::document::ops::ContentKind::Render);
    document
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();
    let original_text = document.buffer().to_string();

    // Perform substitute (global on line)
    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: "g".to_string(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(matches!(result, ExecutionResult::Substituted(_)));
    assert_eq!(document.buffer().to_string(), "baz bar baz");

    // Undo the substitute
    assert!(document.undo());
    assert_eq!(document.buffer().to_string(), original_text);

    // Redo the substitute
    assert!(document.redo());
    assert_eq!(document.buffer().to_string(), "baz bar baz");

    // Undo again
    assert!(document.undo());
    assert_eq!(document.buffer().to_string(), original_text);
}

#[test]
fn test_substitute_whole_file_undo() {
    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();

    // Initial content with multiple lines
    document.replace_content(
        "foo\nbar\nfoo\nbaz\nfoo",
        crate::document::ops::ContentKind::Render,
    );
    document
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();
    let original_text = document.buffer().to_string();

    // Perform whole-file substitute
    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "qux".to_string(),
        flags: "g".to_string(),
        range: Some("%".to_string()),
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(matches!(result, ExecutionResult::Substituted(_)));
    assert_eq!(document.buffer().to_string(), "qux\nbar\nqux\nbaz\nqux");

    // Undo should restore all three substitutions as one operation
    assert!(document.undo());
    assert_eq!(document.buffer().to_string(), original_text);

    // Redo should reapply all three substitutions
    assert!(document.redo());
    assert_eq!(document.buffer().to_string(), "qux\nbar\nqux\nbaz\nqux");
}
#[test]
fn substitute_with_nonempty_set_scopes_to_banked_regions_only() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    document.replace_content(
        "foo bar foo baz foo",
        crate::document::ops::ContentKind::Render,
    );
    // Bank the first and third "foo" (char spans 0..3, 16..19); middle (8..11)
    // is left out. Region::span() is inclusive of cursor, so end = cursor + 1.
    document.bank_region(Region::new(0, 2, RangeKind::Charwise));
    document.bank_region(Region::new(16, 18, RangeKind::Charwise));

    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "XXX".to_string(),
        flags: "g".to_string(),
        range: Some("%".to_string()),
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(matches!(result, ExecutionResult::Substituted(_)));
    assert_eq!(
        document.buffer().to_string(),
        "XXX bar foo baz XXX",
        "middle 'foo' (not banked) must survive"
    );
    assert!(document.selections().is_empty());
}

fn substitute_at_cursor(text: &str, cursor: usize, pattern: &str, replacement: &str) -> String {
    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    document.replace_content(text, crate::document::ops::ContentKind::Render);
    document
        .set_cursor(cursor, crate::document::ops::CursorCause::User)
        .unwrap();

    let command = ParsedCommand::Substitute {
        pattern: pattern.to_string(),
        replacement: replacement.to_string(),
        flags: String::new(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(matches!(result, ExecutionResult::Substituted(_)));
    document.buffer().to_string()
}

#[test]
fn bug_substitute_end_anchor_appends_at_cursor() {
    assert_eq!(substitute_at_cursor("abc", 0, "$", ";"), "abc;");
}

#[test]
fn bug_substitute_start_anchor_prepends_at_cursor() {
    assert_eq!(substitute_at_cursor("abc", 2, "^", "> "), "> abc");
}

#[test]
fn test_execute_undotree() {
    let mut state = State::new();
    let command = ParsedCommand::UndoTree { bangs: 0 };

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert_eq!(
        result,
        ExecutionResult::OpenUndoTree,
        "Expected OpenUndoTree result for UndoTree, got {result:?}"
    );
}

#[test]
fn test_execute_file() {
    let mut state = State::new();
    let command = ParsedCommand::File {
        path: None,
        bangs: 0,
    };

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert!(
        matches!(result, ExecutionResult::OpenDirectory { .. }),
        "Expected OpenDirectory result for :file, got {result:?}"
    );
}

#[test]
fn test_execute_file_path_fails() {
    let mut state = State::new();
    let file_path = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let command = ParsedCommand::File {
        path: Some(file_path),
        bangs: 0,
    };

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();
    let result = CommandExecutor::execute(command, &mut state, &mut document);

    assert_eq!(result, ExecutionResult::Failure);
    assert!(state
        .notifications
        .iter_active()
        .any(|n| n.message.contains("Not a directory")));
}

#[test]
fn test_execute_split() {
    use crate::command_line::commands::types::SplitSubcommand;
    use crate::split::tree::SplitDirection;

    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();

    let result = CommandExecutor::execute(
        ParsedCommand::Split {
            subcommand: SplitSubcommand::Current,
            bangs: 0,
        },
        &mut state,
        &mut document,
    );
    assert_eq!(
        result,
        ExecutionResult::SplitWindow {
            direction: SplitDirection::Horizontal,
            subcommand: SplitSubcommand::Current,
        }
    );
}

#[test]
fn test_execute_vsplit_file() {
    use crate::command_line::commands::types::SplitSubcommand;
    use crate::split::tree::SplitDirection;

    let mut state = State::new();

    let mut document = Document::new(crate::document::DocumentId::new(1)).unwrap();

    let result = CommandExecutor::execute(
        ParsedCommand::VSplit {
            subcommand: SplitSubcommand::File("test.rs".to_string()),
            bangs: 0,
        },
        &mut state,
        &mut document,
    );
    assert_eq!(
        result,
        ExecutionResult::SplitWindow {
            direction: SplitDirection::Vertical,
            subcommand: SplitSubcommand::File("test.rs".to_string()),
        }
    );
}
