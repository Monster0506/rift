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
    UndoTree,
    Normal,
    Insert,
    Command,
    Search,
    FileExplorer,
    /// Clipboard ring index buffer context
    Clipboard,
    /// Clipboard entry scratch buffer context
    ClipboardEntry,
    /// Terminal buffer context — falls through to Insert for passthrough key handling.
    Terminal,
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

    /// Remove a sequence binding. Returns `true` if a binding was removed.
    pub fn unregister_sequence(&mut self, context: KeyContext, keys: &[Key]) -> bool {
        if let Some(trie) = self.mappings.get_mut(&context) {
            trie.remove(keys)
        } else {
            false
        }
    }

    pub fn register_from_str(&mut self, context: KeyContext, key: Key, action_str: &str) {
        if let Ok(action) = Action::from_str(action_str) {
            self.register(context, key, action);
        } else {
            // Log error? For now ignore or could return Result
            eprintln!("Failed to parse action string: {}", action_str);
        }
    }

    /// Returns the parent context for fallback lookup.
    /// `FileExplorer` and `UndoTree` fall through to `Normal` so standard vim motions
    /// work without re-registering every binding.
    /// `Terminal` falls through to `Insert` since it is always in input mode.
    fn parent_context(context: KeyContext) -> Option<KeyContext> {
        match context {
            KeyContext::FileExplorer
            | KeyContext::UndoTree
            | KeyContext::Clipboard
            | KeyContext::ClipboardEntry => Some(KeyContext::Normal),
            KeyContext::Terminal => Some(KeyContext::Global),
            KeyContext::Normal | KeyContext::Insert | KeyContext::Command | KeyContext::Search => {
                Some(KeyContext::Global)
            }
            KeyContext::Global => None,
        }
    }

    /// Look up a key sequence, walking the fallback chain.
    pub fn lookup<'a>(&'a self, context: KeyContext, keys: &[Key]) -> MatchResult<'a> {
        let mut ctx = context;
        loop {
            if let Some(trie) = self.mappings.get(&ctx) {
                match trie.lookup(keys) {
                    MatchResult::None => {}
                    match_result => return match_result,
                }
            }
            match Self::parent_context(ctx) {
                Some(parent) => ctx = parent,
                None => return MatchResult::None,
            }
        }
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
