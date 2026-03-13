use super::*;
use crate::command_line::commands::{ExecutionResult, ParsedCommand};
use crate::document::definitions::{create_document_settings_registry, WrapMode};
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

fn set_wrap(value: &str) -> (ExecutionResult, DocumentOptions) {
    let registry = create_document_settings_registry();
    let mut state = State::new();
    let mut doc = Document::new(1).unwrap();
    let cmd = ParsedCommand::SetLocal {
        option: "wrap".to_string(),
        value: Some(value.to_string()),
        bangs: 0,
    };
    let result = execute_local_command(cmd, &mut state, &mut doc, &registry);
    (result, doc.options)
}

#[test]
fn setlocal_wrap_zero_disables() {
    let (result, opts) = set_wrap("0");
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(opts.wrap, Some(WrapMode::Off));
}

#[test]
fn setlocal_wrap_literal_column() {
    let (result, opts) = set_wrap("80");
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(opts.wrap, Some(WrapMode::Expr("80".to_string())));
}

#[test]
fn setlocal_wrap_auto() {
    let (result, opts) = set_wrap("auto");
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(opts.wrap, Some(WrapMode::Expr("auto".to_string())));
}

#[test]
fn setlocal_wrap_auto_minus() {
    let (result, opts) = set_wrap("auto-5");
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(opts.wrap, Some(WrapMode::Expr("auto-5".to_string())));
}

#[test]
fn setlocal_wrap_auto_div_plus() {
    let (result, opts) = set_wrap("auto/2+5");
    assert_eq!(result, ExecutionResult::Redraw);
    assert_eq!(opts.wrap, Some(WrapMode::Expr("auto/2+5".to_string())));
}

#[test]
fn setlocal_wrap_unknown_keyword_fails() {
    let (result, _) = set_wrap("foo+1");
    assert_eq!(result, ExecutionResult::Failure);
}

#[test]
fn setlocal_wrap_div_by_zero_fails() {
    let (result, _) = set_wrap("auto/0");
    assert_eq!(result, ExecutionResult::Failure);
}
