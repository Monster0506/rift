pub(crate) mod completion;
mod definitions;
pub(crate) mod executor;
mod parser;
mod registry;
mod types;

#[cfg(test)]
pub(crate) use definitions::CompletionHint;
pub(crate) use definitions::{CommandDescriptor, COMMANDS};
pub(crate) use executor::{CommandExecutor, ExecutionResult};
pub(crate) use parser::CommandParser;
pub(crate) use registry::{CommandDef, CommandRegistry, MatchResult};
pub(crate) use types::{ParsedCommand, SplitSubcommand};
