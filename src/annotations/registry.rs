//! Dispatch registry resolving (kind, verb) to a handler.
//! Data-only handlers keep activation serializable and IPC-reachable.

use super::kind::{well_known, Kind};
use super::value::Value;
use std::collections::HashMap;

/// A core builtin behavior, interpreted by the editor when activated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Builtin {
    /// Follow a link: open `payload.href`.
    FollowLink,
    /// Flip `payload.checked` and request a re-render.
    ToggleChecked,
    /// Open an `fs.entry`: descend into `payload.name` if a directory, else open it.
    OpenEntry,
}

/// What a resolved handler is. The registry resolves to one of these; the editor
/// executes it. `Lua` means the Lua host holds the function under the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handler {
    Builtin(Builtin),
    Lua,
    /// Run an ex command named by this string.
    Command(String),
    /// Routed to a remote owner (reserved for IPC).
    Remote,
}

/// Maps (kind-or-prefix, verb) to a handler, with namespace-prefix fallback.
#[derive(Default)]
pub struct DispatchRegistry {
    handlers: HashMap<(String, String), Handler>,
}

impl DispatchRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// A registry preloaded with core builtins.
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register("ui.link", "activate", Handler::Builtin(Builtin::FollowLink));
        r.register(
            "ui.checkbox",
            "toggle",
            Handler::Builtin(Builtin::ToggleChecked),
        );
        r.register(
            well_known::FS_ENTRY,
            "activate",
            Handler::Builtin(Builtin::OpenEntry),
        );
        r
    }

    /// Register a handler for a kind (or kind-prefix) and verb. Re-registering the
    /// same key replaces the handler (plugin reload safety, sec 10).
    pub fn register(&mut self, key: impl Into<String>, verb: impl Into<String>, handler: Handler) {
        self.handlers.insert((key.into(), verb.into()), handler);
    }

    /// Remove every handler registered under an exact key (all its verbs).
    pub fn remove_key(&mut self, key: &str) {
        self.handlers.retain(|(k, _), _| k != key);
    }

    /// Drop all Lua-backed handlers (e.g. before a plugin reload). Builtins,
    /// commands, and remote handlers are kept; Lua handlers re-register on reload.
    pub fn clear_lua_handlers(&mut self) {
        self.handlers.retain(|_, h| *h != Handler::Lua);
    }

    /// Resolve a handler for a kind and verb, falling back from the full kind to
    /// its namespace prefix, then namespace, then the global "*" default.
    pub fn resolve(&self, kind: &Kind, verb: &str) -> Option<&Handler> {
        let ns = kind.namespace();
        let candidates = [
            kind.as_str().to_string(),
            format!("{}.", ns),
            ns.to_string(),
            "*".to_string(),
        ];
        for key in candidates {
            if let Some(h) = self.handlers.get(&(key, verb.to_string())) {
                return Some(h);
            }
        }
        None
    }
}

/// Per-kind defaults: presentation applied when an annotation sets none, and a
/// description used as a fallback hover tooltip.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KindDefaults {
    pub presentation: Option<super::Presentation>,
    pub description: Option<String>,
}

/// Per-kind (or per-prefix) defaults resolved with namespace-prefix fallback.
/// Supplies render/hover defaults only; never privileges a kind in storage.
#[derive(Default)]
pub struct KindRegistry {
    defaults: HashMap<String, KindDefaults>,
    /// Bumped on every mutation, for external staleness gates that must not
    /// react to mere lookups.
    generation: u64,
}

impl KindRegistry {
    pub fn new() -> Self {
        Self {
            defaults: HashMap::new(),
            generation: 0,
        }
    }

    /// Monotonic count of mutations to this registry's defaults.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// A registry preloaded with sensible defaults for core kinds.
    pub fn with_core() -> Self {
        use super::presentation::{FaceRef, Presentation, StyleOverride};
        let mut r = Self::new();
        r.set_presentation(
            "lsp.diagnostic",
            Presentation::with_face(FaceRef::new("diag.error")),
        );
        r.set_description("lsp.diagnostic", "diagnostic");
        let mut link = Presentation::with_face(FaceRef::new("link"));
        link.style = Some(StyleOverride {
            underline: true,
            ..Default::default()
        });
        r.set_presentation("ui.link", link);
        r.set_presentation(
            "ui.button",
            Presentation::with_style(StyleOverride {
                reverse: true,
                ..Default::default()
            }),
        );
        r
    }

    pub fn register(&mut self, key: impl Into<String>, defaults: KindDefaults) {
        self.defaults.insert(key.into(), defaults);
        self.generation += 1;
    }

    pub fn set_presentation(&mut self, key: impl Into<String>, presentation: super::Presentation) {
        self.defaults.entry(key.into()).or_default().presentation = Some(presentation);
        self.generation += 1;
    }

    pub fn set_description(&mut self, key: impl Into<String>, description: impl Into<String>) {
        self.defaults.entry(key.into()).or_default().description = Some(description.into());
        self.generation += 1;
    }

    fn resolve(&self, kind: &Kind) -> Option<&KindDefaults> {
        let ns = kind.namespace();
        let candidates = [
            kind.as_str().to_string(),
            format!("{}.", ns),
            ns.to_string(),
            "*".to_string(),
        ];
        candidates.iter().find_map(|k| self.defaults.get(k))
    }

    /// Default presentation for a kind, with prefix fallback.
    pub fn default_presentation(&self, kind: &Kind) -> Option<&super::Presentation> {
        self.resolve(kind).and_then(|d| d.presentation.as_ref())
    }

    /// Default description (hover doc) for a kind, with prefix fallback.
    pub fn default_description(&self, kind: &Kind) -> Option<&str> {
        self.resolve(kind).and_then(|d| d.description.as_deref())
    }
}

/// Builtin: flip `payload.checked`, defaulting a missing/non-bool to `true`.
pub fn toggle_checked(payload: &mut Value) {
    let current = payload
        .get("checked")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    payload.set("checked", Value::Bool(!current));
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod registry_tests;
