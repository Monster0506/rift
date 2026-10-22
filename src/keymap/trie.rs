use crate::action::Action;
use crate::key::Key;
use std::collections::HashMap;

/// Result of looking up a key sequence
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MatchResult<'a> {
    /// Exact match found
    Exact(&'a Action),
    /// Sequence is a valid prefix of multiple bindings but has no action itself
    Prefix,
    /// Sequence is a valid prefix AND has an action itself (e.g. 'd')
    Ambiguous(&'a Action),
    /// No match found
    None,
}

/// How a lookup treats modifiers that carry no text meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Strictness {
    /// Only exact keys match.
    Exact,
    /// An unbound key retries as `Key::fallback`.
    StripModifiers,
}

/// A node in the key sequence trie
#[derive(Debug, Default, Clone)]
pub(crate) struct TrieNode {
    /// Children nodes mapped by key
    children: HashMap<Key, Self>,
    /// Action associated with this sequence (if any)
    action: Option<Action>,
}

impl TrieNode {
    /// Insert a sequence into the trie
    pub(crate) fn insert(&mut self, keys: &[Key], action: Action) {
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

    /// Remove a sequence from the trie. Returns `true` if a binding was removed.
    pub(crate) fn remove(&mut self, keys: &[Key]) -> bool {
        if keys.is_empty() {
            let had = self.action.is_some();
            self.action = None;
            return had;
        }
        let key = keys[0];
        if let Some(child) = self.children.get_mut(&key) {
            let removed = child.remove(&keys[1..]);
            if child.action.is_none() && child.children.is_empty() {
                self.children.remove(&key);
            }
            removed
        } else {
            false
        }
    }

    /// Look up a sequence
    #[must_use]
    pub(crate) fn lookup<'a>(&'a self, keys: &[Key], strictness: Strictness) -> MatchResult<'a> {
        if keys.is_empty() {
            // A node with both an action and children is Ambiguous: the caller
            // waits for more keys, then flushes to this action on timeout.
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

        self.child(&keys[0], strictness)
            .map_or(MatchResult::None, |child| {
                child.lookup(&keys[1..], strictness)
            })
    }

    fn child(&self, key: &Key, strictness: Strictness) -> Option<&Self> {
        if let Some(child) = self.children.get(key) {
            return Some(child);
        }
        match strictness {
            Strictness::Exact => None,
            Strictness::StripModifiers => self.children.get(&key.fallback()?),
        }
    }
}
