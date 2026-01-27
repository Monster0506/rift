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
mod tests {
    use super::*;
    use crate::action::{EditorAction, Motion};

    #[test]
    fn test_register_and_get() {
        let mut map = KeyMap::new();
        map.register(
            KeyContext::Global,
            Key::Char('j'),
            Action::Editor(EditorAction::Move(Motion::Down)),
        );

        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('j')),
            Some(&Action::Editor(EditorAction::Move(Motion::Down)))
        );
        assert_eq!(map.get_action(KeyContext::Global, Key::Char('k')), None);
    }

    #[test]
    fn test_sequence() {
        let mut map = KeyMap::new();
        map.register_sequence(
            KeyContext::Global,
            vec![Key::Char('d'), Key::Char('d')],
            Action::Editor(EditorAction::DeleteLine),
        );

        // Partial match
        assert_eq!(
            map.lookup(KeyContext::Global, &[Key::Char('d')]),
            MatchResult::Prefix
        );

        // Exact match
        assert_eq!(
            map.lookup(KeyContext::Global, &[Key::Char('d'), Key::Char('d')]),
            MatchResult::Exact(&Action::Editor(EditorAction::DeleteLine))
        );

        // No match
        assert_eq!(
            map.lookup(KeyContext::Global, &[Key::Char('x')]),
            MatchResult::None
        );
    }

    #[test]
    fn test_context_fallback() {
        let mut map = KeyMap::new();
        // Global binding
        map.register(
            KeyContext::Global,
            Key::Char('q'),
            Action::Editor(EditorAction::Quit),
        );

        // Specific binding
        map.register(
            KeyContext::FileExplorer,
            Key::Char('j'),
            Action::Editor(EditorAction::Move(Motion::Down)),
        );

        // Test specific context finding specific binding
        assert_eq!(
            map.get_action(KeyContext::FileExplorer, Key::Char('j')),
            Some(&Action::Editor(EditorAction::Move(Motion::Down)))
        );

        // Test specific context falling back to global
        assert_eq!(
            map.get_action(KeyContext::FileExplorer, Key::Char('q')),
            Some(&Action::Editor(EditorAction::Quit))
        );

        // Test global context finding global binding
        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('q')),
            Some(&Action::Editor(EditorAction::Quit))
        );

        // Test global context NOT finding specific binding
        assert_eq!(map.get_action(KeyContext::Global, Key::Char('j')), None);
    }

    #[test]
    fn test_overwrite() {
        let mut map = KeyMap::new();
        map.register(
            KeyContext::Global,
            Key::Char('j'),
            Action::Editor(EditorAction::Move(Motion::Down)),
        );
        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('j')),
            Some(&Action::Editor(EditorAction::Move(Motion::Down)))
        );

        map.register(KeyContext::Global, Key::Char('j'), Action::Noop);
        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('j')),
            Some(&Action::Noop)
        );
    }

    #[test]
    fn test_register_from_str() {
        let mut map = KeyMap::new();
        map.register_from_str(KeyContext::Global, Key::Char('j'), "editor:move_down");
        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('j')),
            Some(&Action::Editor(EditorAction::Move(Motion::Down)))
        );
    }
}
