//! Optional presentation for annotations (design.md sec 8).
//! A hint the compositor composes on top of base color/syntax.

use crate::color::Color;
use serde::{Deserialize, Serialize};

/// A named theme face, resolved against the active theme (e.g. "link", "diag.error").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaceRef(pub String);

impl FaceRef {
    pub fn new(name: impl Into<String>) -> Self {
        FaceRef(name.into())
    }

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
}

/// Inline style overrides; an escape hatch when a named face is not enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StyleOverride {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub underline: bool,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub reverse: bool,
}

impl StyleOverride {
    /// The text attributes carried by this override (color is read separately).
    pub fn attrs(&self) -> crate::layer::CellAttrs {
        crate::layer::CellAttrs {
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
            reverse: self.reverse,
        }
    }
}

/// Display-only virtual text rendered relative to the anchor (not buffer content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adornment {
    pub text: String,
    pub placement: Placement,
    pub face: Option<FaceRef>,
}

impl Adornment {
    pub fn new(text: impl Into<String>, placement: Placement) -> Self {
        Adornment {
            text: text.into(),
            placement,
            face: None,
        }
    }

    pub fn with_face(mut self, face: FaceRef) -> Self {
        self.face = Some(face);
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
    pub fn with_face(face: FaceRef) -> Self {
        Presentation {
            face: Some(face),
            ..Default::default()
        }
    }

    pub fn with_style(style: StyleOverride) -> Self {
        Presentation {
            style: Some(style),
            ..Default::default()
        }
    }

    pub fn with_adornment(mut self, adornment: Adornment) -> Self {
        self.adornment = Some(adornment);
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Resolve a named face to a foreground color against the active syntax colors,
/// with built-in fallbacks for well-known faces so presentation works untheme-aware.
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
        "diag.info" | "diag.hint" => Some(Color::Cyan),
        "link" => Some(Color::Blue),
        "button" => Some(Color::Cyan),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_round_trips_through_json() {
        let p = Presentation::with_face(FaceRef::new("diag.error"))
            .with_adornment(Adornment::new("E", Placement::Trailing))
            .with_priority(5);
        let json = serde_json::to_string(&p).unwrap();
        let back: Presentation = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn style_override_carries_color() {
        let s = StyleOverride {
            fg: Some(Color::Red),
            bold: true,
            ..Default::default()
        };
        let back: StyleOverride =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.fg, Some(Color::Red));
        assert!(back.bold);
    }
}
