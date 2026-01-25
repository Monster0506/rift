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
    // Add other contexts as needed
}

/// KeyMap stores mappings from (Context, Key) -> Action ID
#[derive(Debug, Clone)]
pub struct KeyMap {
    mappings: HashMap<KeyContext, HashMap<Key, Action>>,
}

impl KeyMap {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    /// Register a new key binding
    pub fn register(&mut self, context: KeyContext, key: Key, action: Action) {
        self.mappings
            .entry(context)
            .or_default()
            .insert(key, action);
    }

    pub fn register_from_str(&mut self, context: KeyContext, key: Key, action_str: &str) {
        if let Ok(action) = Action::from_str(action_str) {
            self.register(context, key, action);
        } else {
            // Log error? For now ignore or could return Result
            eprintln!("Failed to parse action string: {}", action_str);
        }
    }

    /// specific context -> global fallback
    pub fn get_action(&self, context: KeyContext, key: Key) -> Option<&Action> {
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
        ); // Just using random action for test

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
