pub mod defaults;
pub mod trie;

pub use self::trie::{MatchResult, TrieNode};
use crate::action::Action;
use crate::key::Key;
use std::collections::HashMap;
use std::str::FromStr;

/// Context where input occurs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyContext {
    Global,
    FileExplorer,
    UndoTree,
    Normal,
    Insert,
    Command,
    Search,
    // Add other contexts as needed
}

/// KeyMap stores mappings from (Context, Key Sequence) -> Action
#[derive(Debug, Clone)]
pub struct KeyMap {
    mappings: HashMap<KeyContext, TrieNode>,
}

impl KeyMap {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    /// Register a new single-key binding
    pub fn register(&mut self, context: KeyContext, key: Key, action: Action) {
        self.register_sequence(context, vec![key], action);
    }

    /// Register a sequence binding
    pub fn register_sequence(&mut self, context: KeyContext, keys: Vec<Key>, action: Action) {
        self.mappings
            .entry(context)
            .or_default()
            .insert(&keys, action);
    }

    pub fn register_from_str(&mut self, context: KeyContext, key: Key, action_str: &str) {
        if let Ok(action) = Action::from_str(action_str) {
            self.register(context, key, action);
        } else {
            // Log error? For now ignore or could return Result
            eprintln!("Failed to parse action string: {}", action_str);
        }
    }

    /// Look up a key sequence
    pub fn lookup<'a>(&'a self, context: KeyContext, keys: &[Key]) -> MatchResult<'a> {
        // First try specific context
        if let Some(trie) = self.mappings.get(&context) {
            match trie.lookup(keys) {
                MatchResult::None => {} // Continue to fallback
                match_result => return match_result,
            }
        }

        // Fallback to Global context if not found in specific context
        if context != KeyContext::Global {
            if let Some(trie) = self.mappings.get(&KeyContext::Global) {
                return trie.lookup(keys);
            }
        }

        MatchResult::None
    }

    /// Legacy single-key compatibility (returns Action only if Exact match on single key)
    pub fn get_action(&self, context: KeyContext, key: Key) -> Option<&Action> {
        match self.lookup(context, &[key]) {
            MatchResult::Exact(action) | MatchResult::Ambiguous(action) => Some(action),
            _ => None,
        }
    }
}

impl Default for KeyMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
