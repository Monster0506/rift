use crate::action::Action;
use crate::key::Key;
use std::collections::HashMap;

/// Result of looking up a key sequence
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchResult<'a> {
    /// Exact match found
    Exact(&'a Action),
    /// Sequence is a valid prefix of multiple bindings but has no action itself
    Prefix,
    /// Sequence is a valid prefix AND has an action itself (e.g. 'd')
    Ambiguous(&'a Action),
    /// No match found
    None,
}

/// A node in the key sequence trie
#[derive(Debug, Default, Clone)]
pub struct TrieNode {
    /// Children nodes mapped by key
    children: HashMap<Key, TrieNode>,
    /// Action associated with this sequence (if any)
    action: Option<Action>,
}

impl TrieNode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a sequence into the trie
    pub fn insert(&mut self, keys: &[Key], action: Action) {
        if keys.is_empty() {
            self.action = Some(action);
            return;
        }

        let key = keys[0];
        self.children
            .entry(key)
            .or_default()
            .insert(&keys[1..], action);
    }

    /// Look up a sequence
    pub fn lookup<'a>(&'a self, keys: &[Key]) -> MatchResult<'a> {
        if keys.is_empty() {
            // We reached the end of the input sequence.
            // If this node has an action, it's an exact match.
            // But if it also has children, it's ambiguous (Prefix).
            // Usually, longest match wins, or specific overrides prefix.
            // If we have an exact match here, we return it.
            // If we have children but no action, it's a Prefix.
            // If we have children AND action, strictly speaking it's an Exact match for the *current* sequence,
            // but the user might type more.
            // Vim logic: if exact match exists, execute it immediately UNLESS there's a longer mapping?
            // Actually, if a mapping is a prefix of another, usually wait for timeout.
            // For simplicity: if Exact match exists, return Exact.
            // If NO exact match but Children exist, return Prefix.

            if let Some(action) = &self.action {
                if !self.children.is_empty() {
                    return MatchResult::Ambiguous(action);
                }
                return MatchResult::Exact(action);
            }
            if !self.children.is_empty() {
                return MatchResult::Prefix;
            }
            return MatchResult::None;
        }

        let key = keys[0];
        if let Some(child) = self.children.get(&key) {
            child.lookup(&keys[1..])
        } else {
            MatchResult::None
        }
    }
}
