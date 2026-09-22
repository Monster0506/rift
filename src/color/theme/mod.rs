//! Theme system
//! Provides predefined color themes for the editor

use super::Color;
use crate::state::UserSettings;

/// Theme handler trait, allowing themes to be extended to apply more than
/// just background/foreground colors (status bar, selection, cursor, etc.)
pub trait ThemeHandler {
    /// Apply a theme to the given settings; called whenever a theme is
    /// changed via `:set theme <name>`.
    fn apply_theme(&self, theme: &Theme, settings: &mut UserSettings);
}

/// Default theme handler; applies background and foreground colors, and can
/// be extended to handle additional theme properties as they're added.
pub struct DefaultThemeHandler;

impl ThemeHandler for DefaultThemeHandler {
    fn apply_theme(&self, theme: &Theme, settings: &mut UserSettings) {
        settings.theme = Some(theme.name.to_string());
        settings.editor_bg = Some(theme.background);
        settings.editor_fg = Some(theme.foreground);
        settings.syntax_colors = theme.syntax.clone();
        settings.cursor_color = Some(theme.cursor_color);
    }
}

/// Global theme handler instance
/// In the future, this could be made configurable or passed as a parameter
static THEME_HANDLER: DefaultThemeHandler = DefaultThemeHandler;

use std::collections::HashMap;

/// Syntax highlighting colors for a theme
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxColors {
    pub colors: HashMap<String, Color>,
}

impl SyntaxColors {
    /// Create syntax colors from a list of base colors
    pub fn from_base_colors(base: &[(&str, Color)]) -> Self {
        Self {
            colors: base.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        }
    }

    pub fn get_color(&self, capture: &str) -> Option<Color> {
        // 1. Normalize (remove leading @ and trim)
        let clean_capture = capture.trim_start_matches('@').trim();

        // 2. Try exact match
        if let Some(color) = self.colors.get(clean_capture) {
            return Some(*color);
        }

        // 3. Fallback components (e.g., "function.builtin" -> "function")
        let mut part = clean_capture;
        while let Some(dot_index) = part.rfind('.') {
            part = &part[0..dot_index];
            if let Some(color) = self.colors.get(part) {
                return Some(*color);
            }
        }

        // 4. Return None to let caller fall back to editor foreground
        None
    }
}

/// Editor theme definition
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme name
    pub name: &'static str,
    /// Light/dark classification, exposed for consumers that adapt to it;
    /// the built-in renderer keys off explicit colors, not this field.
    pub variant: ThemeVariant,
    /// Background color
    pub background: Color,
    /// Foreground (text) color
    pub foreground: Color,
    /// Cursor accent color (block fill in Normal mode, bar color in Insert mode)
    pub cursor_color: Color,
    /// Optional syntax highlighting colors
    pub syntax: Option<SyntaxColors>,
}

/// Theme variant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeVariant {
    Light,
    Dark,
}

impl Theme {
    /// Create a new theme
    #[must_use]
    pub fn new(
        name: &'static str,
        variant: ThemeVariant,
        background: Color,
        foreground: Color,
        cursor_color: Color,
        syntax: Option<SyntaxColors>,
    ) -> Self {
        Theme {
            name,
            variant,
            background,
            foreground,
            cursor_color,
            syntax,
        }
    }

    /// Get the light theme
    #[must_use]
    pub fn light() -> Self {
        use crate::constants::captures::*;
        let fg = Color::Black;
        let def = Color::Rgb {
            r: 29,
            g: 91,
            b: 143,
        };
        let string = Color::Rgb {
            r: 46,
            g: 125,
            b: 50,
        };
        let num = Color::Rgb {
            r: 14,
            g: 124,
            b: 134,
        };
        let comment = Color::Rgb {
            r: 110,
            g: 122,
            b: 110,
        };
        let punct = Color::Rgb {
            r: 105,
            g: 109,
            b: 114,
        };
        let variable = Color::Rgb {
            r: 74,
            g: 64,
            b: 58,
        };
        let syntax = SyntaxColors::from_base_colors(&[
            (KEYWORD, fg),
            (FUNCTION, def),
            (TYPE, def),
            (STRING, string),
            (NUMBER, num),
            (CONSTANT, num),
            (BOOLEAN, num),
            (COMMENT, comment),
            (VARIABLE, variable),
            (PARAMETER, variable),
            (PROPERTY, fg),
            (ATTRIBUTE, fg),
            ("ui.lsp.ok", Color::DarkBlue),
            ("ui.lsp.error", Color::DarkRed),
            (
                "ui.lsp.warn",
                Color::Rgb {
                    r: 175,
                    g: 100,
                    b: 0,
                },
            ),
            (NAMESPACE, def),
            (OPERATOR, fg),
            (PUNCTUATION, punct),
            (CONSTRUCTOR, def),
            (BUILTIN, def),
            (TEXT_TITLE, def),
            (TEXT_LITERAL, string),
            (TEXT_REFERENCE, def),
            (TEXT_URI, string),
            (TAG, def),
            (LABEL, fg),
            (ESCAPE, num),
            ("method", def),
            ("conditional", fg),
            ("repeat", fg),
            ("preproc", fg),
            ("delimiter", fg),
            ("embedded", fg),
            ("none", fg),
            ("charset", fg),
            ("import", fg),
            ("keyframes", fg),
            ("media", fg),
            ("supports", fg),
            ("field", fg),
            (C_IMPORT, fg),
            (CHARACTER, string),
            (MODULE_BUILTIN, def),
            (SPELL, comment),
            (MODULE, def),
        ]);

        Theme::new(
            crate::constants::themes::LIGHT,
            ThemeVariant::Light,
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            Color::Rgb { r: 0, g: 0, b: 0 },
            Color::Rgb {
                r: 0,
                g: 120,
                b: 212,
            },
            Some(syntax),
        )
    }

    /// Get the dark theme
    #[must_use]
    pub fn dark() -> Self {
        use crate::constants::captures::*;
        let fg = Color::Rgb {
            r: 224,
            g: 224,
            b: 224,
        };
        let def = Color::Rgb {
            r: 111,
            g: 179,
            b: 224,
        };
        let string = Color::Rgb {
            r: 143,
            g: 203,
            b: 130,
        };
        let num = Color::Rgb {
            r: 95,
            g: 214,
            b: 196,
        };
        let comment = Color::Rgb {
            r: 138,
            g: 155,
            b: 138,
        };
        let punct = Color::Rgb {
            r: 136,
            g: 144,
            b: 160,
        };
        let variable = Color::Rgb {
            r: 214,
            g: 201,
            b: 184,
        };
        let syntax = SyntaxColors::from_base_colors(&[
            (KEYWORD, fg),
            (FUNCTION, def),
            (TYPE, def),
            (STRING, string),
            (NUMBER, num),
            (CONSTANT, num),
            (BOOLEAN, num),
            (COMMENT, comment),
            (VARIABLE, variable),
            (PARAMETER, variable),
            (PROPERTY, fg),
            (ATTRIBUTE, fg),
            (NAMESPACE, def),
            (OPERATOR, fg),
            (PUNCTUATION, punct),
            (CONSTRUCTOR, def),
            ("ui.lsp.ok", Color::Cyan),
            ("ui.lsp.error", Color::Red),
            ("ui.lsp.warn", Color::Yellow),
            (BUILTIN, def),
            (TEXT_TITLE, def),
            (TEXT_LITERAL, string),
            (TEXT_REFERENCE, def),
            (TEXT_URI, string),
            (TAG, def),
            (LABEL, fg),
            (ESCAPE, num),
            ("method", def),
            ("conditional", fg),
            ("repeat", fg),
            ("preproc", fg),
            ("delimiter", fg),
            ("embedded", fg),
            ("none", fg),
            ("charset", fg),
            ("import", fg),
            ("keyframes", fg),
            ("media", fg),
            ("supports", fg),
            ("field", fg),
            (C_IMPORT, fg),
            (CHARACTER, string),
            (MODULE_BUILTIN, def),
            (SPELL, comment),
            (MODULE, def),
        ]);

        Theme::new(
            crate::constants::themes::DARK,
            ThemeVariant::Dark,
            Color::Rgb {
                r: 30,
                g: 30,
                b: 30,
            },
            Color::Rgb {
                r: 224,
                g: 224,
                b: 224,
            },
            Color::Rgb {
                r: 86,
                g: 156,
                b: 214,
            },
            Some(syntax),
        )
    }

    /// Get the gruvbox theme (dark variant)
    #[must_use]
    pub fn gruvbox() -> Self {
        use crate::constants::captures::*;
        let fg = Color::Rgb {
            r: 235,
            g: 219,
            b: 178,
        };
        let def = Color::Rgb {
            r: 142,
            g: 192,
            b: 124,
        };
        let string = Color::Rgb {
            r: 184,
            g: 187,
            b: 38,
        };
        let num = Color::Rgb {
            r: 212,
            g: 135,
            b: 156,
        };
        let comment = Color::Rgb {
            r: 146,
            g: 131,
            b: 116,
        };
        let punct = Color::Rgb {
            r: 168,
            g: 153,
            b: 132,
        };
        let variable = Color::Rgb {
            r: 195,
            g: 210,
            b: 213,
        };
        let syntax = SyntaxColors::from_base_colors(&[
            (KEYWORD, fg),
            (FUNCTION, def),
            (TYPE, def),
            (STRING, string),
            (NUMBER, num),
            (CONSTANT, num),
            (BOOLEAN, num),
            (COMMENT, comment),
            (VARIABLE, variable),
            (PARAMETER, variable),
            (PROPERTY, fg),
            (ATTRIBUTE, fg),
            (NAMESPACE, def),
            (OPERATOR, fg),
            (PUNCTUATION, punct),
            (CONSTRUCTOR, def),
            (BUILTIN, def),
            (TEXT_TITLE, def),
            (TEXT_LITERAL, string),
            (TEXT_REFERENCE, def),
            (TEXT_URI, string),
            (TAG, def),
            (LABEL, fg),
            (ESCAPE, num),
            ("method", def),
            ("conditional", fg),
            ("repeat", fg),
            ("preproc", fg),
            ("delimiter", fg),
            ("embedded", fg),
            ("none", fg),
            ("charset", fg),
            ("import", fg),
            ("keyframes", fg),
            ("media", fg),
            ("supports", fg),
            ("field", fg),
            (C_IMPORT, fg),
            (CHARACTER, string),
            (MODULE_BUILTIN, def),
            (SPELL, comment),
            (MODULE, def),
            (
                "ui.lsp.ok",
                Color::Rgb {
                    r: 131,
                    g: 165,
                    b: 152,
                },
            ),
            (
                "ui.lsp.error",
                Color::Rgb {
                    r: 251,
                    g: 73,
                    b: 52,
                },
            ),
            (
                "ui.lsp.warn",
                Color::Rgb {
                    r: 250,
                    g: 189,
                    b: 47,
                },
            ),
        ]);

        Theme::new(
            crate::constants::themes::GRUVBOX,
            ThemeVariant::Dark,
            Color::Rgb {
                r: 40,
                g: 40,
                b: 32,
            },
            fg,
            Color::Rgb {
                r: 255,
                g: 146,
                b: 47,
            },
            Some(syntax),
        )
    }

    /// Get the nordic theme
    #[must_use]
    pub fn nordic() -> Self {
        use crate::constants::captures::*;
        let fg = Color::Rgb {
            r: 187,
            g: 195,
            b: 212,
        };
        let def = Color::Rgb {
            r: 136,
            g: 192,
            b: 208,
        };
        let string = Color::Rgb {
            r: 163,
            g: 190,
            b: 140,
        };
        let num = Color::Rgb {
            r: 180,
            g: 142,
            b: 173,
        };
        let comment = Color::Rgb {
            r: 129,
            g: 161,
            b: 193,
        };
        let punct = Color::Rgb {
            r: 123,
            g: 136,
            b: 161,
        };
        let variable = Color::Rgb {
            r: 216,
            g: 222,
            b: 233,
        };
        let syntax = SyntaxColors::from_base_colors(&[
            (KEYWORD, fg),
            (FUNCTION, def),
            (TYPE, def),
            (STRING, string),
            (NUMBER, num),
            (CONSTANT, num),
            (BOOLEAN, num),
            (COMMENT, comment),
            (VARIABLE, variable),
            (PARAMETER, variable),
            (PROPERTY, def),
            (ATTRIBUTE, fg),
            (NAMESPACE, def),
            (OPERATOR, fg),
            (PUNCTUATION, punct),
            (CONSTRUCTOR, def),
            (BUILTIN, def),
            (TEXT_TITLE, def),
            (TEXT_LITERAL, string),
            (TEXT_REFERENCE, def),
            (TEXT_URI, string),
            (TAG, def),
            (LABEL, fg),
            (ESCAPE, num),
            ("method", def),
            ("conditional", fg),
            ("repeat", fg),
            ("preproc", fg),
            ("delimiter", fg),
            ("embedded", fg),
            ("none", fg),
            ("charset", fg),
            ("import", fg),
            ("keyframes", fg),
            ("media", fg),
            ("supports", fg),
            (C_IMPORT, fg),
            (CHARACTER, string),
            (MODULE_BUILTIN, def),
            (SPELL, comment),
            (MODULE, def),
            (
                "ui.lsp.ok",
                Color::Rgb {
                    r: 143,
                    g: 188,
                    b: 187,
                },
            ),
            (
                "ui.lsp.error",
                Color::Rgb {
                    r: 191,
                    g: 97,
                    b: 106,
                },
            ),
            (
                "ui.lsp.warn",
                Color::Rgb {
                    r: 235,
                    g: 203,
                    b: 139,
                },
            ),
        ]);

        Theme::new(
            crate::constants::themes::NORDIC,
            ThemeVariant::Dark,
            Color::Rgb {
                r: 46,
                g: 52,
                b: 64,
            },
            fg,
            Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
            Some(syntax),
        )
    }

    /// Get theme by name
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            crate::constants::themes::LIGHT => Some(Theme::light()),
            crate::constants::themes::DARK => Some(Theme::dark()),
            crate::constants::themes::GRUVBOX => Some(Theme::gruvbox()),
            crate::constants::themes::NORDIC => Some(Theme::nordic()),
            _ => None,
        }
    }

    /// Get all available theme names
    #[must_use]
    pub fn available_themes() -> Vec<&'static str> {
        vec![
            crate::constants::themes::LIGHT,
            crate::constants::themes::DARK,
            crate::constants::themes::GRUVBOX,
            crate::constants::themes::NORDIC,
        ]
    }

    /// Apply this theme using the default theme handler
    /// This is the main entry point for applying themes
    pub fn apply_to_settings(&self, settings: &mut UserSettings) {
        THEME_HANDLER.apply_theme(self, settings);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
