//! Tests for command executor

use crate::command_line::commands::{CommandExecutor, ExecutionResult, ParsedCommand};
use crate::command_line::settings::create_settings_registry;
use crate::document::definitions::create_document_settings_registry;
use crate::document::Document;
use crate::state::State;

#[test]
fn test_execute_quit() {
    let mut state = State::new();
    let command = ParsedCommand::Quit { bangs: 0 };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Quit { bangs: 0 });
}

#[test]
fn test_execute_set_expandtabs_true() {
    let mut state = State::new();
    assert_eq!(state.settings.expand_tabs, true); // Default

    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("true".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, true);
}

#[test]
fn test_execute_set_expandtabs_false() {
    let mut state = State::new();
    assert_eq!(state.settings.expand_tabs, true); // Default

    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("false".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, false);
}

#[test]
fn test_execute_set_expandtabs_alias_et() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "et".to_string(),
        value: Some("false".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, false);
}

#[test]
fn test_execute_set_expandtabs_boolean_variants() {
    let mut state = State::new();

    // Test "on"
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("on".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, true);

    // Test "off"
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("off".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, false);

    // Test "yes"
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("yes".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, true);

    // Test "no"
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("no".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, false);

    // Test "1"
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("1".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, true);

    // Test "0"
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("0".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, false);
}

#[test]
fn test_execute_set_expandtabs_case_insensitive() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "EXPANDTABS".to_string(),
        value: Some("FALSE".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, false);
}

#[test]
fn test_execute_set_tabwidth() {
    let mut state = State::new();
    assert_eq!(state.settings.tab_width, 4); // Default

    let command = ParsedCommand::Set {
        option: "tabwidth".to_string(),
        value: Some("4".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.tab_width, 4);
}

#[test]
fn test_execute_set_tabwidth_alias_tw() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "tw".to_string(),
        value: Some("2".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.tab_width, 2);
}

#[test]
fn test_execute_set_tabwidth_various_values() {
    let mut state = State::new();

    // Test various tab widths
    for width in &[1, 2, 4, 8, 16, 32] {
        let command = ParsedCommand::Set {
            option: "tabwidth".to_string(),
            value: Some(width.to_string()),
            bangs: 0,
        };

        let settings_registry = create_settings_registry();
        let document_settings_registry = create_document_settings_registry();
        let mut document = Document::new(1).unwrap();
        let result = CommandExecutor::execute(
            command,
            &mut state,
            &mut document,
            &settings_registry,
            &document_settings_registry,
        );
        assert_eq!(result, ExecutionResult::Redraw);
        assert_eq!(state.settings.tab_width, *width);
    }
}

#[test]
fn test_execute_set_tabwidth_zero_error() {
    let mut state = State::new();
    let original_width = state.settings.tab_width;

    let command = ParsedCommand::Set {
        option: "tabwidth".to_string(),
        value: Some("0".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Failure);

    // Check if error was reported to manager
    let notifications: Vec<_> = state.error_manager.notifications().iter_active().collect();
    assert_eq!(notifications.len(), 1);
    assert!(
        notifications[0]
            .message
            .contains("tabwidth must be greater than 0")
            || notifications[0].message.contains("is below minimum")
    );

    // State should not be modified
    assert_eq!(state.settings.tab_width, original_width);
}

#[test]
fn test_execute_set_tabwidth_invalid_number() {
    let mut state = State::new();
    let original_width = state.settings.tab_width;

    let command = ParsedCommand::Set {
        option: "tabwidth".to_string(),
        value: Some("invalid".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Failure);

    let notifications: Vec<_> = state.error_manager.notifications().iter_active().collect();
    assert_eq!(notifications.len(), 1);
    assert!(
        notifications[0].message.contains("Invalid integer")
            || notifications[0].message.contains("Parse error")
            || notifications[0].message.contains("Invalid numeric")
    );

    // State should not be modified
    assert_eq!(state.settings.tab_width, original_width);
}

#[test]
fn test_execute_set_expandtabs_invalid_boolean() {
    let mut state = State::new();
    let original_value = state.settings.expand_tabs;

    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("maybe".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Failure);

    let notifications: Vec<_> = state.error_manager.notifications().iter_active().collect();
    assert_eq!(notifications.len(), 1);
    assert!(notifications[0].message.contains("Invalid boolean value"));

    // State should not be modified
    assert_eq!(state.settings.expand_tabs, original_value);
}

#[test]
fn test_execute_set_expandtabs_missing_value() {
    let mut state = State::new();
    let original_value = state.settings.expand_tabs;

    // When value is None, it should be treated as "true" by the parser
    // But the executor expects a value, so this should error
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: None,
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    match result {
        ExecutionResult::Failure => {
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("Missing value")));
        }
        _ => panic!("Expected error for missing value"),
    }

    // State should not be modified
    assert_eq!(state.settings.expand_tabs, original_value);
}

#[test]
fn test_execute_set_unknown_option() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "unknownoption".to_string(),
        value: Some("value".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    match result {
        ExecutionResult::Failure => {
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("Unknown option")));
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("unknownoption")));
        }
        _ => panic!("Expected error for unknown option"),
    }
}

#[test]
fn test_execute_unknown_command() {
    let mut state = State::new();

    let command = ParsedCommand::Unknown {
        name: "nonexistent".to_string(),
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    match result {
        ExecutionResult::Failure => {
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("Unknown command")));
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("nonexistent")));
        }
        _ => panic!("Expected error for unknown command"),
    }
}

#[test]
fn test_execute_ambiguous_command() {
    let mut state = State::new();

    let command = ParsedCommand::Ambiguous {
        prefix: "se".to_string(),
        matches: vec!["setup".to_string(), "settings".to_string()],
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    match result {
        ExecutionResult::Failure => {
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("Ambiguous command")));
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("se")));
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("setup")));
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("settings")));
        }
        _ => panic!("Expected error for ambiguous command"),
    }
}

#[test]
fn test_execute_set_multiple_options() {
    let mut state = State::new();

    // Set expandtabs to false
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("false".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, false);

    // Set tabwidth to 4
    let command = ParsedCommand::Set {
        option: "tabwidth".to_string(),
        value: Some("4".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.tab_width, 4);

    // Set expandtabs back to true
    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("true".to_string()),
        bangs: 0,
    };
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.expand_tabs, true);

    // Verify both settings are correct
    assert_eq!(state.settings.expand_tabs, true);
    assert_eq!(state.settings.tab_width, 4);
}

#[test]
fn test_execute_set_tabwidth_large_value() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "tabwidth".to_string(),
        value: Some("100".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(state.settings.tab_width, 100);
}

#[test]
fn test_execute_set_tabwidth_negative_error() {
    let mut state = State::new();
    let original_width = state.settings.tab_width;

    // Try to set negative value (will fail to parse as usize)
    let command = ParsedCommand::Set {
        option: "tabwidth".to_string(),
        value: Some("-1".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    match result {
        ExecutionResult::Failure => {
            assert!(
                state
                    .error_manager
                    .notifications()
                    .iter_active()
                    .any(|n| n.message.contains("Invalid integer"))
                    || state
                        .error_manager
                        .notifications()
                        .iter_active()
                        .any(|n| n.message.contains("Parse error"))
                    || state
                        .error_manager
                        .notifications()
                        .iter_active()
                        .any(|n| n.message.contains("Invalid numeric"))
            );
        }
        _ => panic!("Expected error for negative number"),
    }

    // State should not be modified
    assert_eq!(state.settings.tab_width, original_width);
}

#[test]
fn test_execute_set_expandtabs_empty_string() {
    let mut state = State::new();
    let original_value = state.settings.expand_tabs;

    let command = ParsedCommand::Set {
        option: "expandtabs".to_string(),
        value: Some("".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    match result {
        ExecutionResult::Failure => {
            assert!(state
                .error_manager
                .notifications()
                .iter_active()
                .any(|n| n.message.contains("Invalid boolean value")));
        }
        _ => panic!("Expected error for empty string"),
    }

    // State should not be modified
    assert_eq!(state.settings.expand_tabs, original_value);
}

#[test]
fn test_execute_set_case_insensitive_option_names() {
    let mut state = State::new();

    // Test various case combinations
    let cases = vec![
        ("EXPANDTABS", "false"),
        ("ExpandTabs", "true"),
        ("expandTABS", "false"),
        ("TABWIDTH", "16"),
        ("TabWidth", "4"),
        ("tabWIDTH", "8"),
    ];

    for (option, value) in cases {
        let command = ParsedCommand::Set {
            option: option.to_string(),
            value: Some(value.to_string()),
            bangs: 0,
        };

        let settings_registry = create_settings_registry();
        let document_settings_registry = create_document_settings_registry();
        let mut document = Document::new(1).unwrap();
        let result = CommandExecutor::execute(
            command,
            &mut state,
            &mut document,
            &settings_registry,
            &document_settings_registry,
        );
        assert_eq!(result, ExecutionResult::Redraw);
    }

    // Final state
    assert_eq!(state.settings.expand_tabs, false);
    assert_eq!(state.settings.tab_width, 8);
}

#[test]
fn test_execute_set_tabwidth_float_error() {
    let mut state = State::new();
    let original_width = state.settings.tab_width;

    let command = ParsedCommand::Set {
        option: "tabwidth".to_string(),
        value: Some("4.5".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    match result {
        ExecutionResult::Failure => {
            assert!(
                state
                    .error_manager
                    .notifications()
                    .iter_active()
                    .any(|n| n.message.contains("Invalid integer"))
                    || state
                        .error_manager
                        .notifications()
                        .iter_active()
                        .any(|n| n.message.contains("Parse error"))
                    || state
                        .error_manager
                        .notifications()
                        .iter_active()
                        .any(|n| n.message.contains("Invalid numeric"))
            );
        }
        _ => panic!("Expected error for float"),
    }

    // State should not be modified
    assert_eq!(state.settings.tab_width, original_width);
}

#[test]
fn test_execute_write_no_path() {
    let mut state = State::new();
    let command = ParsedCommand::Write {
        path: None,
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Success);
    assert!(state.file_path.is_none());
}

#[test]
fn test_execute_write_with_path() {
    let mut state = State::new();
    let command = ParsedCommand::Write {
        path: Some("test.txt".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Success);
    assert_eq!(state.file_path, Some("test.txt".to_string()));
}

#[test]
fn test_execute_write_updates_path() {
    let mut state = State::new();
    state.set_file_path(Some("old.txt".to_string()));

    let command = ParsedCommand::Write {
        path: Some("new.txt".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::Success);
    assert_eq!(state.file_path, Some("new.txt".to_string()));
}

#[test]
fn test_execute_write_quit_no_path() {
    let mut state = State::new();
    let command = ParsedCommand::WriteQuit {
        path: None,
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::WriteAndQuit);
    assert!(state.file_path.is_none());
}

#[test]
fn test_execute_write_quit_with_path() {
    let mut state = State::new();
    let command = ParsedCommand::WriteQuit {
        path: Some("test.txt".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::WriteAndQuit);
    assert_eq!(state.file_path, Some("test.txt".to_string()));
}

#[test]
fn test_execute_write_quit_updates_path() {
    let mut state = State::new();
    state.set_file_path(Some("old.txt".to_string()));

    let command = ParsedCommand::WriteQuit {
        path: Some("new.txt".to_string()),
        bangs: 0,
    };

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(result, ExecutionResult::WriteAndQuit);
    assert_eq!(state.file_path, Some("new.txt".to_string()));
}
