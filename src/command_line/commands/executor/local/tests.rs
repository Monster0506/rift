use super::*;
use crate::command_line::commands::{ExecutionResult, ParsedCommand};
use crate::document::definitions::create_document_settings_registry;
use crate::document::Document;
use crate::state::State;

#[test]
fn test_execute_setlocal_line_ending() {
    let mut state = State::new();
    // Default is LF

    let command = ParsedCommand::SetLocal {
        option: "line_ending".to_string(),
        value: Some("crlf".to_string()),
        bangs: 0,
    };

    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();

    // Verify default
    use crate::document::LineEnding;
    assert_eq!(document.options.line_ending, LineEnding::LF);

    let result = execute_local_command(
        command,
        &mut state,
        &mut document,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Success);
    assert_eq!(document.options.line_ending, LineEnding::CRLF);
}
