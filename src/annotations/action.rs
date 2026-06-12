//! Interaction descriptors for annotations (design.md sec 9).
//! Serializable: an annotation says what can be done, never how (no closures).

use super::kind::Kind;
use super::value::Value;
use super::AnnotationId;
use serde::{Deserialize, Serialize};

/// A suggested key binding shown in affordance UI (e.g. "Enter", "t").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyHint(pub String);

impl KeyHint {
    pub fn new(s: impl Into<String>) -> Self {
        KeyHint(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A description of an interaction. The handler is resolved at activation time by
/// the dispatch registry keyed on (kind, verb) - never stored on the annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// "activate" (default), "toggle", "stage", "discard", etc.
    pub verb: String,
    /// Bound to the generic activate key.
    pub default: bool,
    /// Suggested binding shown in affordance UI.
    pub key_hint: Option<KeyHint>,
    /// Serializable args passed to the handler.
    pub params: Value,
}

impl Action {
    pub fn new(verb: impl Into<String>) -> Self {
        Action {
            verb: verb.into(),
            default: false,
            key_hint: None,
            params: Value::Null,
        }
    }

    /// A default "activate" action bound to the generic activate key.
    pub fn activate() -> Self {
        Action {
            verb: "activate".to_string(),
            default: true,
            key_hint: None,
            params: Value::Null,
        }
    }

    pub fn as_default(mut self) -> Self {
        self.default = true;
        self
    }

    pub fn with_key_hint(mut self, hint: KeyHint) -> Self {
        self.key_hint = Some(hint);
        self
    }

    pub fn with_params(mut self, params: Value) -> Self {
        self.params = params;
        self
    }
}

/// The event built when an interactive annotation is activated and routed to its
/// owner's handler. Fully serializable so a remote owner only changes transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivationEvent {
    pub annotation_id: AnnotationId,
    pub kind: Kind,
    pub verb: String,
    pub params: Value,
    /// Byte offset the activation happened at.
    pub position: usize,
    /// Document/buffer id the annotation belongs to.
    pub buffer: u64,
    pub document_version: u64,
}
