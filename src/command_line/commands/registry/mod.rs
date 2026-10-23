//! Command registry
//! Manages command definitions and provides intelligent matching with aliases and prefix matching

/// Result of matching a command input
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MatchResult {
    /// Exact match found (command name or explicit alias)
    Exact(String),
    /// Shortest unambiguous prefix match
    Prefix(String),
    /// Ambiguous - multiple commands match
    Ambiguous {
        prefix: String,
        matches: Vec<String>,
    },
    /// No match found
    Unknown(String),
}

/// Command definition
#[derive(Debug, Clone)]
pub(crate) struct CommandDef {
    /// Canonical command name
    name: String,
    /// Explicit aliases for this command
    aliases: Vec<String>,
    /// Description of the command
    description: String,
    /// Subcommands registry
    pub(super) subcommands: Option<CommandRegistry>,
}

impl CommandDef {
    /// Create a new command definition
    pub(in crate::command_line) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            description: String::new(),
            subcommands: None,
        }
    }

    /// Add a description
    #[must_use]
    pub(super) fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Add an explicit alias
    #[must_use]
    pub(in crate::command_line) fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    /// Add multiple explicit aliases
    #[must_use]
    pub(super) fn with_aliases(mut self, aliases: Vec<impl Into<String>>) -> Self {
        for alias in aliases {
            self.aliases.push(alias.into());
        }
        self
    }

    /// Add a subcommand
    #[must_use]
    pub(super) fn with_subcommand(mut self, cmd: Self) -> Self {
        let mut registry = self.subcommands.unwrap_or_default();
        registry = registry.register(cmd);
        self.subcommands = Some(registry);
        self
    }
}

/// Command registry
#[derive(Debug, Clone)]
pub(crate) struct CommandRegistry {
    commands: Vec<CommandDef>,
}

impl CommandRegistry {
    /// Create a new empty registry
    #[must_use]
    pub(in crate::command_line) const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Register a command
    #[must_use]
    pub(in crate::command_line) fn register(mut self, cmd: CommandDef) -> Self {
        self.commands.push(cmd);
        self
    }

    /// Match an input string to a command: exact name/alias, then shortest
    /// unambiguous prefix, else ambiguous (multiple matches) or unknown.
    #[must_use]
    pub(in crate::command_line) fn match_command(&self, input: &str) -> MatchResult {
        let input = input.trim();
        let input_lower = input.to_lowercase();

        if input.is_empty() {
            return MatchResult::Unknown(input.to_string());
        }

        // Aliases are matched case-sensitively so "H"/"L" and "h"/"l" stay distinct.
        for cmd in &self.commands {
            for alias in &cmd.aliases {
                if *alias == input {
                    return MatchResult::Exact(cmd.name.clone());
                }
            }
        }

        for cmd in &self.commands {
            if cmd.name.to_lowercase() == input_lower {
                return MatchResult::Exact(cmd.name.clone());
            }
        }

        let mut matches = Vec::new();
        for cmd in &self.commands {
            if cmd.name.to_lowercase().starts_with(&input_lower) {
                matches.push(cmd.name.clone());
            }
            for alias in &cmd.aliases {
                if alias.starts_with(input) && !matches.contains(&cmd.name) {
                    matches.push(cmd.name.clone());
                }
            }
        }

        match matches.len() {
            0 => MatchResult::Unknown(input.to_string()),
            1 => MatchResult::Prefix(matches[0].clone()),
            _ => {
                for cmd in &self.commands {
                    for alias in &cmd.aliases {
                        if *alias == input {
                            return MatchResult::Exact(cmd.name.clone());
                        }
                    }
                }
                MatchResult::Ambiguous {
                    prefix: input.to_string(),
                    matches,
                }
            }
        }
    }

    #[cfg(test)]
    /// Get all registered command names
    #[must_use]
    fn command_names(&self) -> Vec<&String> {
        self.commands.iter().map(|c| &c.name).collect()
    }

    /// Get a command definition by canonical name
    #[must_use]
    pub(super) fn get(&self, name: &str) -> Option<&CommandDef> {
        self.commands.iter().find(|c| c.name == name)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
