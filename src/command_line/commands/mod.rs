pub mod definitions;
pub mod executor;
pub mod parser;
pub mod registry;
pub mod types;

pub use definitions::{CommandDescriptor, COMMANDS};
pub use executor::{CommandExecutor, ExecutionResult};
pub use parser::CommandParser;
pub use registry::{CommandDef, CommandRegistry, MatchResult};
pub use types::ParsedCommand;
