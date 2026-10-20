//! Optional presentation for annotations.
//! A hint the compositor composes on top of base color/syntax.

use crate::color::Color;
use crate::layer::CellAttrs;
use serde::{Deserialize, Serialize};

/// A named theme face, resolved against the active theme (e.g. "link", "diag.error").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaceRef(pub String);

impl FaceRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where a display-only adornment renders relative to its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    /// Before the anchor.
    Leading,
    /// After the anchor (e.g. an end-of-line diagnostic message).
    Trailing,
    /// Over the anchor (e.g. button chrome).
    Overlay,
    /// Hide the anchored range: zero display width, following text reflows left
    /// (e.g. markdown emphasis markers). Revealed on the cursor's own line.
    Conceal,
}

/// Inline style overrides; an escape hatch when a named face is not enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
// Serialized as flat JSON keys; grouping would change the wire format.
pub struct StyleOverride {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    #[serde(default)]
    pub attrs: CellAttrs,
}

/// Display-only virtual text rendered relative to the anchor (not buffer content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adornment {
    pub text: String,
    pub placement: Placement,
    pub face: Option<FaceRef>,
    /// Inline color override, preferred over `face` and any kind default.
    pub style: Option<StyleOverride>,
}

impl Adornment {
    pub fn new(text: impl Into<String>, placement: Placement) -> Self {
        Self {
            text: text.into(),
            placement,
            face: None,
            style: None,
        }
    }

    #[must_use]
    pub fn with_face(mut self, face: FaceRef) -> Self {
        self.face = Some(face);
        self
    }

    #[must_use]
    pub const fn with_style(mut self, style: StyleOverride) -> Self {
        self.style = Some(style);
        self
    }
}

/// Presentation hint composed over base color/syntax. All fields optional.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Presentation {
    pub face: Option<FaceRef>,
    pub style: Option<StyleOverride>,
    pub adornment: Option<Adornment>,
    /// Overlap precedence; higher wins when annotations style the same cell.
    pub priority: i32,
}

impl Presentation {
    #[must_use]
    pub fn with_face(face: FaceRef) -> Self {
        Self {
            face: Some(face),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn with_style(style: StyleOverride) -> Self {
        Self {
            style: Some(style),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn with_adornment(mut self, adornment: Adornment) -> Self {
        self.adornment = Some(adornment);
        self
    }

    #[must_use]
    pub const fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Resolve a named face to a foreground color against the active syntax colors,
/// with built-in fallbacks for well-known faces so presentation works untheme-aware.
#[must_use]
pub fn resolve_face(
    face: &FaceRef,
    colors: Option<&crate::color::theme::SyntaxColors>,
) -> Option<Color> {
    if let Some(c) = colors.and_then(|s| s.get_color(face.as_str())) {
        return Some(c);
    }
    match face.as_str() {
        "diag.error" => Some(Color::Red),
        "diag.warning" => Some(Color::Yellow),
        "diag.info" | "diag.hint" | "button" => Some(Color::Cyan),
        "link" => Some(Color::Blue),
        _ => None,
    }
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod presentation_tests;
