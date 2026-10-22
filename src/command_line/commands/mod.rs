pub(crate) mod completion;
pub(crate) mod definitions;
pub(crate) mod executor;
pub(crate) mod parser;
pub(crate) mod registry;
pub(crate) mod types;

#[cfg(test)]
pub(crate) use definitions::CompletionHint;
pub(crate) use definitions::{CommandDescriptor, COMMANDS};
pub(crate) use executor::{CommandExecutor, ExecutionResult};
pub(crate) use parser::CommandParser;
pub(crate) use registry::{CommandDef, CommandRegistry, MatchResult};
pub(crate) use types::{ParsedCommand, SplitSubcommand};
