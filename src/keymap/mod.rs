pub(crate) mod defaults;
pub(crate) mod trie;

use crate::action::Action;
use crate::document::runtime::BufferKindId;
use crate::key::Key;
use crate::keymap::trie::{MatchResult, Strictness, TrieNode};
use std::collections::{BTreeMap, HashMap};

/// Trait for resolving the parent context of a given key context.
pub(crate) trait ParentResolver {
    fn resolve_parent(&mut self, context: KeyContext) -> Option<KeyContext>;
}

impl<F> ParentResolver for F
where
    F: FnMut(KeyContext) -> Option<KeyContext>,
{
    fn resolve_parent(&mut self, context: KeyContext) -> Option<KeyContext> {
        self(context)
    }
}

/// Context where input occurs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyContext {
    Global,
    Normal,
    /// Operator-pending mode: used after d/c/y. Falls through to Normal so
    /// all motions remain available.
    OperatorPending,
    Insert,
    Command,
    Search,
    /// Terminal buffer in insert mode: falls through to Global only (no
    /// Normal bindings should intercept typed characters).
    Terminal,
    /// Terminal buffer in normal mode: falls through to Normal so all vim motions work.
    TerminalNormal,
    /// Visual/VisualLine/VisualBlock selection. Falls through to `Normal` so
    /// every motion remains available without re-registering it.
    Visual,
    /// Runtime buffer kind context
    Buffer(BufferKindId),
}

/// Opaque token identifying a key binding owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct KeyBindingToken(pub(crate) u64);

impl KeyBindingToken {
    /// Create a new token from a raw identifier.
    #[must_use]
    pub(crate) const fn new(raw: u64) -> Self {
        Self(raw)
    }
}

/// A single registration layer entry for a sequence.
#[derive(Debug, Clone)]
struct LayerEntry {
    owner: Option<KeyBindingToken>,
    action: Action,
}

/// `KeyMap` stores mappings from (Context, Key Sequence) -> Action with layered registrations.
#[derive(Debug, Clone)]
pub struct KeyMap {
    /// Active lookup trie per context for fast traversal on the input hot path.
    mappings: HashMap<KeyContext, TrieNode>,
    /// Stack of registration layers for each sequence, ordered from oldest to newest.
    layers: BTreeMap<(KeyContext, Vec<Key>), Vec<LayerEntry>>,
}

impl KeyMap {
    #[must_use]
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
            layers: BTreeMap::new(),
        }
    }

    /// Register a new single-key binding.
    pub fn register(&mut self, context: KeyContext, key: Key, action: Action) {
        self.register_sequence(context, vec![key], action);
    }

    /// Register a sequence binding.
    pub(crate) fn register_sequence(
        &mut self,
        context: KeyContext,
        keys: Vec<Key>,
        action: Action,
    ) {
        self.register_layered(context, keys, action, None);
    }

    /// Register a sequence binding with an explicit owner token.
    pub(crate) fn register_with_owner(
        &mut self,
        context: KeyContext,
        keys: Vec<Key>,
        action: Action,
        owner: KeyBindingToken,
    ) {
        self.register_layered(context, keys, action, Some(owner));
    }

    /// Register a layered sequence binding with an optional owner token.
    fn register_layered(
        &mut self,
        context: KeyContext,
        keys: Vec<Key>,
        action: Action,
        owner: Option<KeyBindingToken>,
    ) {
        let entry = LayerEntry {
            owner,
            action: action.clone(),
        };
        self.mappings
            .entry(context)
            .or_default()
            .insert(&keys, action);
        self.layers.entry((context, keys)).or_default().push(entry);
    }

    /// Refresh the active trie node for a sequence after layers change.
    fn refresh_binding(&mut self, context: KeyContext, keys: &[Key]) {
        let key_tuple = (context, keys.to_vec());
        let top_action = self
            .layers
            .get(&key_tuple)
            .and_then(|stack| stack.last().map(|entry| entry.action.clone()));
        if let Some(action) = top_action {
            self.mappings
                .entry(context)
                .or_default()
                .insert(keys, action);
        } else {
            self.layers.remove(&key_tuple);
            if let Some(trie) = self.mappings.get_mut(&context) {
                trie.remove(keys);
            }
        }
    }

    /// Remove the top registration layer for a sequence, restoring any lower layer.
    /// Returns `true` if a layer or binding was removed.
    pub(crate) fn unregister_sequence(&mut self, context: KeyContext, keys: &[Key]) -> bool {
        let key_tuple = (context, keys.to_vec());
        let mut removed = false;
        if let Some(stack) = self.layers.get_mut(&key_tuple) {
            if stack.pop().is_some() {
                removed = true;
            }
        }
        if removed {
            self.refresh_binding(context, keys);
            true
        } else if let Some(trie) = self.mappings.get_mut(&context) {
            trie.remove(keys)
        } else {
            false
        }
    }

    /// Remove all registration layers belonging to an owner token, restoring any lower layers.
    pub(crate) fn unregister_owner(&mut self, owner: KeyBindingToken) -> usize {
        let mut count = 0;
        let mut to_cleanup = Vec::new();

        for ((ctx, keys), stack) in &mut self.layers {
            let initial_len = stack.len();
            stack.retain(|entry| {
                if entry.owner == Some(owner) {
                    count += 1;
                    false
                } else {
                    true
                }
            });
            if stack.len() != initial_len {
                to_cleanup.push((*ctx, keys.clone()));
            }
        }

        for (ctx, keys) in to_cleanup {
            self.refresh_binding(ctx, &keys);
        }

        count
    }

    #[cfg(test)]
    /// Return the active registration layer count for a sequence.
    #[must_use]
    fn layer_count(&self, context: KeyContext, keys: &[Key]) -> usize {
        self.layers
            .get(&(context, keys.to_vec()))
            .map_or(0, std::vec::Vec::len)
    }

    /// Static fallback chain for built-in contexts; `Buffer` kinds have no
    /// built-in fallback and must be resolved by the caller.
    #[must_use]
    pub(crate) const fn parent_context(context: KeyContext) -> Option<KeyContext> {
        match context {
            KeyContext::OperatorPending | KeyContext::Visual | KeyContext::TerminalNormal => {
                Some(KeyContext::Normal)
            }
            KeyContext::Terminal => Some(KeyContext::Global),
            KeyContext::Normal | KeyContext::Insert | KeyContext::Command | KeyContext::Search => {
                Some(KeyContext::Global)
            }
            KeyContext::Global | KeyContext::Buffer(_) => None,
        }
    }

    /// Looks up a sequence via the fallback chain; `resolve_parent` overrides
    /// buffer-context parents, built-in contexts keep their static fallback.
    ///
    /// Exact keys win across the whole chain; only then are modifiers with
    /// no text meaning stripped (`Key::fallback`) and the chain retried.
    pub(crate) fn lookup_with_parent<'a, R: ParentResolver>(
        &'a self,
        context: KeyContext,
        keys: &[Key],
        mut resolve_parent: R,
    ) -> MatchResult<'a> {
        crate::perf_span!("keymap_lookup", crate::perf::PerfFields::default());
        let exact = self.lookup_chain(context, keys, Strictness::Exact, &mut resolve_parent);
        if exact != MatchResult::None || keys.iter().all(|key| key.fallback().is_none()) {
            return exact;
        }
        self.lookup_chain(
            context,
            keys,
            Strictness::StripModifiers,
            &mut resolve_parent,
        )
    }

    fn lookup_chain<'a, R: ParentResolver>(
        &'a self,
        context: KeyContext,
        keys: &[Key],
        strictness: Strictness,
        resolve_parent: &mut R,
    ) -> MatchResult<'a> {
        let mut ctx = context;
        let mut depth = 0;
        loop {
            if let Some(trie) = self.mappings.get(&ctx) {
                match trie.lookup(keys, strictness) {
                    MatchResult::None => {}
                    match_result => return match_result,
                }
            }
            // Guard against cyclic fallback chains.
            if depth >= 16 {
                return MatchResult::None;
            }
            depth += 1;
            let next = match ctx {
                KeyContext::Buffer(_) => resolve_parent.resolve_parent(ctx),
                other => resolve_parent
                    .resolve_parent(other)
                    .or_else(|| Self::parent_context(other)),
            };
            match next {
                Some(parent) => {
                    ctx = parent;
                }
                None => {
                    return MatchResult::None;
                }
            }
        }
    }

    #[cfg(test)]
    /// Look up a key sequence, walking the default static fallback chain.
    pub(crate) fn lookup<'a>(&'a self, context: KeyContext, keys: &[Key]) -> MatchResult<'a> {
        self.lookup_with_parent(context, keys, Self::parent_context)
    }

    /// Single-key compatibility lookup with caller-supplied parent resolution.
    fn get_action_with_parent<R: ParentResolver>(
        &self,
        context: KeyContext,
        key: Key,
        resolve_parent: R,
    ) -> Option<&Action> {
        match self.lookup_with_parent(context, &[key], resolve_parent) {
            MatchResult::Exact(action) | MatchResult::Ambiguous(action) => Some(action),
            _ => None,
        }
    }

    /// Legacy single-key compatibility using default static fallback.
    pub fn get_action(&self, context: KeyContext, key: Key) -> Option<&Action> {
        self.get_action_with_parent(context, key, Self::parent_context)
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
