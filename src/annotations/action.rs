//! Interaction descriptors for annotations.
//! Serializable: an annotation says what can be done, never how (no closures).

use super::value::Value;
use serde::{Deserialize, Serialize};

/// A suggested key binding shown in affordance UI (e.g. "Enter", "t").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct KeyHint(pub String);

impl KeyHint {
    #[cfg(test)]
    pub(crate) fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A description of an interaction. The handler is resolved at activation time by
/// the dispatch registry keyed on (kind, verb) - never stored on the annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// "activate" (default), "toggle", "stage", "discard", etc.
    pub(crate) verb: String,
    /// Bound to the generic activate key.
    pub(crate) default: bool,
    /// Suggested binding shown in affordance UI.
    pub(crate) key_hint: Option<KeyHint>,
    /// Serializable args passed to the handler.
    pub(crate) params: Value,
}

impl Action {
    pub(crate) fn new(verb: impl Into<String>) -> Self {
        Self {
            verb: verb.into(),
            default: false,
            key_hint: None,
            params: Value::Null,
        }
    }

    /// A default "activate" action bound to the generic activate key.
    #[must_use]
    pub fn activate() -> Self {
        Self {
            verb: "activate".to_string(),
            default: true,
            key_hint: None,
            params: Value::Null,
        }
    }

    #[must_use]
    pub(crate) const fn with_default(mut self) -> Self {
        self.default = true;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_key_hint(mut self, hint: KeyHint) -> Self {
        self.key_hint = Some(hint);
        self
    }
}
