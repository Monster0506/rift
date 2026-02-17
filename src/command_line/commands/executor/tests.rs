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
fn test_execute_set_number_true() {
    let mut state = State::new();
    state.settings.show_line_numbers = false;

    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, true);
}

#[test]
fn test_execute_set_number_false() {
    let mut state = State::new();
    assert_eq!(state.settings.show_line_numbers, true); // Default

    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, false);
}

#[test]
fn test_execute_set_clminwidth_alias() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "clminwidth".to_string(),
        value: Some("50".to_string()),
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
    assert_eq!(state.settings.command_line_window.min_width, 50);
}

#[test]
fn test_execute_set_number_boolean_variants() {
    let mut state = State::new();

    // Test "on"
    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, true);

    // Test "off"
    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, false);

    // Test "yes"
    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, true);

    // Test "no"
    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, false);

    // Test "1"
    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, true);

    // Test "0"
    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, false);
}

#[test]
fn test_execute_set_number_case_insensitive() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "NUMBER".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, false);
}

#[test]
fn test_execute_set_clminwidth() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "command_line.min_width".to_string(),
        value: Some("60".to_string()),
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
    assert_eq!(state.settings.command_line_window.min_width, 60);
}

#[test]
fn test_execute_set_clminwidth_various_values() {
    let mut state = State::new();

    // Test various widths
    for width in &[10, 20, 40, 80, 100] {
        let command = ParsedCommand::Set {
            option: "clminwidth".to_string(),
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
        assert_eq!(result, ExecutionResult::Success);
        assert_eq!(state.settings.command_line_window.min_width, *width);
    }
}

#[test]
fn test_execute_set_clminwidth_zero_error() {
    let mut state = State::new();
    let original_width = state.settings.command_line_window.min_width;

    let command = ParsedCommand::Set {
        option: "clminwidth".to_string(),
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
    assert!(notifications[0].message.contains("is below minimum"));

    // State should not be modified
    assert_eq!(state.settings.command_line_window.min_width, original_width);
}

#[test]
fn test_execute_set_clminwidth_invalid_number() {
    let mut state = State::new();
    let original_width = state.settings.command_line_window.min_width;

    let command = ParsedCommand::Set {
        option: "clminwidth".to_string(),
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
    assert_eq!(state.settings.command_line_window.min_width, original_width);
}

#[test]
fn test_execute_set_number_invalid_boolean() {
    let mut state = State::new();
    let original_value = state.settings.show_line_numbers;

    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, original_value);
}

#[test]
fn test_execute_set_number_missing_value() {
    let mut state = State::new();
    let original_value = state.settings.show_line_numbers;

    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, original_value);
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

    // Set number to false
    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, false);

    // Set clminwidth to 50
    let command = ParsedCommand::Set {
        option: "clminwidth".to_string(),
        value: Some("50".to_string()),
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
    assert_eq!(state.settings.command_line_window.min_width, 50);

    // Set number back to true
    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, true);

    // Verify both settings are correct
    assert_eq!(state.settings.show_line_numbers, true);
    assert_eq!(state.settings.command_line_window.min_width, 50);
}

#[test]
fn test_execute_set_clminwidth_large_value() {
    let mut state = State::new();

    let command = ParsedCommand::Set {
        option: "clminwidth".to_string(),
        value: Some("1000".to_string()),
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
    assert_eq!(state.settings.command_line_window.min_width, 1000);
}

#[test]
fn test_execute_set_clminwidth_negative_error() {
    let mut state = State::new();
    let original_width = state.settings.command_line_window.min_width;

    // Try to set negative value (will fail to parse as usize)
    let command = ParsedCommand::Set {
        option: "clminwidth".to_string(),
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
    assert_eq!(state.settings.command_line_window.min_width, original_width);
}

#[test]
fn test_execute_set_number_empty_string() {
    let mut state = State::new();
    let original_value = state.settings.show_line_numbers;

    let command = ParsedCommand::Set {
        option: "number".to_string(),
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
    assert_eq!(state.settings.show_line_numbers, original_value);
}

#[test]
fn test_execute_set_case_insensitive_option_names() {
    let mut state = State::new();

    // Test various case combinations
    let cases = vec![
        ("NUMBER", "false"),
        ("NumBer", "true"),
        ("numBER", "false"),
        ("CLMINWIDTH", "16"),
        ("cLmInWiDtH", "4"),
        ("clminWIDTH", "8"),
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
        // number returns Redraw, clminwidth returns Success
        if option.to_lowercase() == "number" {
            assert_eq!(result, ExecutionResult::Redraw);
        } else {
            assert_eq!(result, ExecutionResult::Success);
        }
    }

    // Final state
    assert_eq!(state.settings.show_line_numbers, false);
    assert_eq!(state.settings.command_line_window.min_width, 8);
}

#[test]
fn test_execute_set_clminwidth_float_error() {
    let mut state = State::new();
    let original_width = state.settings.command_line_window.min_width;

    let command = ParsedCommand::Set {
        option: "clminwidth".to_string(),
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
    assert_eq!(state.settings.command_line_window.min_width, original_width);
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
    assert_eq!(result, ExecutionResult::Write);
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
    assert_eq!(result, ExecutionResult::Write);
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
    assert_eq!(result, ExecutionResult::Write);
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

#[test]
fn test_substitute_current_line() {
    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    document.buffer.insert_str("foo bar foo").unwrap();
    // Cursor at 0
    document.buffer.set_cursor(0).unwrap();

    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: "".to_string(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(document.buffer.to_string(), "baz bar foo");
}

#[test]
fn test_substitute_global_line() {
    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    document.buffer.insert_str("foo bar foo").unwrap();
    document.buffer.set_cursor(0).unwrap();

    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: "g".to_string(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(document.buffer.to_string(), "baz bar baz");
}

#[test]
fn test_substitute_whole_file() {
    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    document.buffer.insert_str("foo\nbar\nfoo").unwrap();
    document.buffer.set_cursor(0).unwrap();

    // :s%/foo/baz/g
    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: "g".to_string(),
        range: Some("%".to_string()),
        bangs: 0,
    };

    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(document.buffer.to_string(), "baz\nbar\nbaz");
}

#[test]
fn test_substitute_case_insensitive() {
    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    document.buffer.insert_str("Foo bar foo").unwrap();
    document.buffer.set_cursor(0).unwrap();

    // Smart case: lowercase pattern "foo" matches both "Foo" and "foo" (case-insensitive)
    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: "".to_string(), // subst flags
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(document.buffer.to_string(), "baz bar foo"); // First match only (no 'g' flag)
}

#[test]
fn test_substitute_no_match() {
    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();
    document.buffer.insert_str("hello world").unwrap();

    let command = ParsedCommand::Substitute {
        pattern: "nothere".to_string(),
        replacement: "baz".to_string(),
        flags: "".to_string(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Failure);
    assert_eq!(document.buffer.revision, 1); // Only insert incremented it
    assert!(state
        .error_manager
        .notifications()
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
        ParsedCommand::Unknown { name } => {
            assert_eq!(name, "s/foo/bar");
        }
        _ => panic!("Expected Unknown command, got {:?}", command),
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
        _ => panic!("Expected Substitute command, got {:?}", command),
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
        _ => panic!("Expected Substitute command, got {:?}", command),
    }
}

#[test]
fn test_substitute_undo_redo() {
    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();

    // Initial content
    document.buffer.insert_str("foo bar foo").unwrap();
    document.buffer.set_cursor(0).unwrap();
    let original_text = document.buffer.to_string();

    // Perform substitute (global on line)
    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "baz".to_string(),
        flags: "g".to_string(),
        range: None,
        bangs: 0,
    };

    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(document.buffer.to_string(), "baz bar baz");

    // Undo the substitute
    assert!(document.undo());
    assert_eq!(document.buffer.to_string(), original_text);

    // Redo the substitute
    assert!(document.redo());
    assert_eq!(document.buffer.to_string(), "baz bar baz");

    // Undo again
    assert!(document.undo());
    assert_eq!(document.buffer.to_string(), original_text);
}

#[test]
fn test_substitute_whole_file_undo() {
    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();

    // Initial content with multiple lines
    document
        .buffer
        .insert_str("foo\nbar\nfoo\nbaz\nfoo")
        .unwrap();
    document.buffer.set_cursor(0).unwrap();
    let original_text = document.buffer.to_string();

    // Perform whole-file substitute
    let command = ParsedCommand::Substitute {
        pattern: "foo".to_string(),
        replacement: "qux".to_string(),
        flags: "g".to_string(),
        range: Some("%".to_string()),
        bangs: 0,
    };

    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(document.buffer.to_string(), "qux\nbar\nqux\nbaz\nqux");

    // Undo should restore all three substitutions as one operation
    assert!(document.undo());
    assert_eq!(document.buffer.to_string(), original_text);

    // Redo should reapply all three substitutions
    assert!(document.redo());
    assert_eq!(document.buffer.to_string(), "qux\nbar\nqux\nbaz\nqux");
}
#[test]
fn test_execute_undotree() {
    let mut state = State::new();
    let command = ParsedCommand::UndoTree { bangs: 0 };

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

    if let ExecutionResult::OpenComponent {
        component: _,
        initial_job,
        initial_message: _,
    } = result
    {
        assert!(initial_job.is_none());
    } else {
        panic!(
            "Expected OpenComponent result for UndoTree, got {:?}",
            result
        );
    }
}

#[test]
fn test_execute_explore() {
    let mut state = State::new();
    let command = ParsedCommand::Explore {
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

    if let ExecutionResult::OpenComponent {
        component: _,
        initial_job,
        initial_message: _,
    } = result
    {
        assert!(
            initial_job.is_some(),
            "Explore should have an initial job (listing)"
        );
    } else {
        panic!(
            "Expected OpenComponent result for Explore, got {:?}",
            result
        );
    }
}

#[test]
fn test_execute_split() {
    use crate::command_line::commands::SplitSubcommand;
    use crate::split::tree::SplitDirection;

    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();

    let result = CommandExecutor::execute(
        ParsedCommand::Split { subcommand: SplitSubcommand::Current, bangs: 0 },
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
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
    use crate::command_line::commands::SplitSubcommand;
    use crate::split::tree::SplitDirection;

    let mut state = State::new();
    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();

    let result = CommandExecutor::execute(
        ParsedCommand::VSplit {
            subcommand: SplitSubcommand::File("test.rs".to_string()),
            bangs: 0,
        },
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );
    assert_eq!(
        result,
        ExecutionResult::SplitWindow {
            direction: SplitDirection::Vertical,
            subcommand: SplitSubcommand::File("test.rs".to_string()),
        }
    );
}
