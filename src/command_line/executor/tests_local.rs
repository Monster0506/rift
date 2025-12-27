#[cfg(test)]
use crate::command_line::executor::{CommandExecutor, ExecutionResult};
#[cfg(test)]
use crate::command_line::parser::ParsedCommand;
#[cfg(test)]
use crate::command_line::settings::create_settings_registry;
#[cfg(test)]
use crate::document::settings::create_document_settings_registry;
#[cfg(test)]
use crate::document::Document;
#[cfg(test)]
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

    let settings_registry = create_settings_registry();
    let document_settings_registry = create_document_settings_registry();
    let mut document = Document::new(1).unwrap();

    // Verify default
    use crate::document::LineEnding;
    assert_eq!(document.options.line_ending, LineEnding::LF);

    let result = CommandExecutor::execute(
        command,
        &mut state,
        &mut document,
        &settings_registry,
        &document_settings_registry,
    );

    assert_eq!(result, ExecutionResult::Success);
    assert_eq!(document.options.line_ending, LineEnding::CRLF);
}
