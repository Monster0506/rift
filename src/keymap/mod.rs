use crate::key::Key;
use std::collections::HashMap;

/// Context where input occurs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyContext {
    Global,
    FileExplorer,
    Normal,
    Insert,
    // Add other contexts as needed
}

/// KeyMap stores mappings from (Context, Key) -> Action ID
#[derive(Debug, Clone)]
pub struct KeyMap {
    mappings: HashMap<KeyContext, HashMap<Key, String>>,
}

impl KeyMap {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    /// Register a new key binding
    pub fn register(&mut self, context: KeyContext, key: Key, action: &str) {
        self.mappings
            .entry(context)
            .or_default()
            .insert(key, action.to_string());
    }

    /// specific context -> global fallback
    pub fn get_action(&self, context: KeyContext, key: Key) -> Option<&str> {
        // First try specific context
        if let Some(map) = self.mappings.get(&context) {
            if let Some(action) = map.get(&key) {
                return Some(action);
            }
        }

        // Fallback to Global context if not found in specific context
        if context != KeyContext::Global {
            if let Some(map) = self.mappings.get(&KeyContext::Global) {
                if let Some(action) = map.get(&key) {
                    return Some(action);
                }
            }
        }

        None
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

    #[test]
    fn test_register_and_get() {
        let mut map = KeyMap::new();
        map.register(KeyContext::Global, Key::Char('j'), "move_down");

        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('j')),
            Some("move_down")
        );
        assert_eq!(map.get_action(KeyContext::Global, Key::Char('k')), None);
    }

    #[test]
    fn test_context_fallback() {
        let mut map = KeyMap::new();
        // Global binding
        map.register(KeyContext::Global, Key::Char('q'), "quit");

        // Specific binding
        map.register(KeyContext::FileExplorer, Key::Char('j'), "next_item");

        // Test specific context finding specific binding
        assert_eq!(
            map.get_action(KeyContext::FileExplorer, Key::Char('j')),
            Some("next_item")
        );

        // Test specific context falling back to global
        assert_eq!(
            map.get_action(KeyContext::FileExplorer, Key::Char('q')),
            Some("quit")
        );

        // Test global context finding global binding
        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('q')),
            Some("quit")
        );

        // Test global context NOT finding specific binding
        assert_eq!(map.get_action(KeyContext::Global, Key::Char('j')), None);
    }

    #[test]
    fn test_overwrite() {
        let mut map = KeyMap::new();
        map.register(KeyContext::Global, Key::Char('j'), "move_down");
        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('j')),
            Some("move_down")
        );

        map.register(KeyContext::Global, Key::Char('j'), "custom_action");
        assert_eq!(
            map.get_action(KeyContext::Global, Key::Char('j')),
            Some("custom_action")
        );
    }
}
