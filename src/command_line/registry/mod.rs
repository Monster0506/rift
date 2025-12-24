//! Command registry
//! Manages command definitions and provides intelligent matching with aliases and prefix matching

/// Result of matching a command input
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchResult {
    /// Exact match found (command name or explicit alias)
    Exact(String),
    /// Shortest unambiguous prefix match
    Prefix(String),
    /// Ambiguous - multiple commands match
    Ambiguous { prefix: String, matches: Vec<String> },
    /// No match found
    Unknown(String),
}

/// Command definition
#[derive(Debug, Clone)]
pub struct CommandDef {
    /// Canonical command name
    pub name: String,
    /// Explicit aliases for this command
    pub aliases: Vec<String>,
}

impl CommandDef {
    /// Create a new command definition
    pub fn new(name: impl Into<String>) -> Self {
        CommandDef {
            name: name.into(),
            aliases: Vec::new(),
        }
    }

    /// Add an explicit alias
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    /// Add multiple explicit aliases
    pub fn with_aliases(mut self, aliases: Vec<impl Into<String>>) -> Self {
        for alias in aliases {
            self.aliases.push(alias.into());
        }
        self
    }
}

/// Command registry
pub struct CommandRegistry {
    commands: Vec<CommandDef>,
}

impl CommandRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        CommandRegistry {
            commands: Vec::new(),
        }
    }

    /// Register a command
    pub fn register(mut self, cmd: CommandDef) -> Self {
        self.commands.push(cmd);
        self
    }

    /// Register multiple commands
    pub fn register_all(mut self, cmds: Vec<CommandDef>) -> Self {
        self.commands.extend(cmds);
        self
    }

    /// Match an input string to a command
    /// 
    /// Matching order:
    /// 1. Exact match against command name or explicit alias
    /// 2. Check if input is an explicit alias
    /// 3. Shortest unambiguous prefix match
    /// 4. Return ambiguous if multiple matches
    /// 5. Return unknown if no match
    pub fn match_command(&self, input: &str) -> MatchResult {
        let input = input.trim().to_lowercase();
        
        if input.is_empty() {
            return MatchResult::Unknown(input);
        }

        // Step 1: Check exact matches (command names and aliases)
        for cmd in &self.commands {
            if cmd.name.to_lowercase() == input {
                return MatchResult::Exact(cmd.name.clone());
            }
            for alias in &cmd.aliases {
                if alias.to_lowercase() == input {
                    return MatchResult::Exact(cmd.name.clone());
                }
            }
        }

        // Step 2: Check if input matches any explicit alias exactly
        // (This is redundant with step 1, but we'll keep it for clarity)
        
        // Step 3: Find all commands that start with the input prefix
        let mut matches = Vec::new();
        
        for cmd in &self.commands {
            let cmd_lower = cmd.name.to_lowercase();
            if cmd_lower.starts_with(&input) {
                matches.push(cmd.name.clone());
            }
            
            // Also check aliases
            for alias in &cmd.aliases {
                let alias_lower = alias.to_lowercase();
                if alias_lower.starts_with(&input) {
                    // Only add if not already in matches (avoid duplicates)
                    if !matches.contains(&cmd.name) {
                        matches.push(cmd.name.clone());
                    }
                }
            }
        }

        // Step 4: Handle results
        match matches.len() {
            0 => MatchResult::Unknown(input),
            1 => MatchResult::Prefix(matches[0].clone()),
            _ => {
                // Multiple matches - check if any explicit alias matches exactly
                // This allows explicit aliases to override prefix matching
                for cmd in &self.commands {
                    for alias in &cmd.aliases {
                        if alias.to_lowercase() == input {
                            return MatchResult::Exact(cmd.name.clone());
                        }
                    }
                }
                // Still ambiguous
                MatchResult::Ambiguous {
                    prefix: input,
                    matches,
                }
            }
        }
    }

    /// Get all registered command names
    pub fn command_names(&self) -> Vec<&String> {
        self.commands.iter().map(|c| &c.name).collect()
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
