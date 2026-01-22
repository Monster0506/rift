//! Theme system
//! Provides predefined color themes for the editor

use super::Color;
use crate::state::UserSettings;

/// Theme handler trait for applying themes
///
/// This trait allows themes to be extended to apply more than just background/foreground colors.
/// In the future, themes can include:
/// - Status bar colors
/// - Selection highlight colors
/// - Cursor colors
/// - Command line window colors
/// - Border colors
/// - Syntax highlighting colors
/// - etc.
pub trait ThemeHandler {
    /// Apply a theme to the given settings
    ///
    /// This is called whenever a theme is changed via `:set theme <name>`.
    /// The handler is responsible for applying all theme properties to the settings.
    fn apply_theme(&self, theme: &Theme, settings: &mut UserSettings);
}

/// Default theme handler implementation
///
/// Currently applies background and foreground colors, but can be extended
/// to handle additional theme properties as they are added to the Theme struct.
///
/// Example of future extension:
/// ```rust,ignore
/// impl ThemeHandler for DefaultThemeHandler {
///     fn apply_theme(&self, theme: &Theme, settings: &mut UserSettings) {
///         settings.theme = Some(theme.name.to_string());
///         settings.editor_bg = Some(theme.background);
///         settings.editor_fg = Some(theme.foreground);
///         settings.status_bar_bg = Some(theme.status_bar_bg);
///         settings.status_bar_fg = Some(theme.status_bar_fg);
///         settings.selection_bg = Some(theme.selection_bg);
///         settings.cursor_color = Some(theme.cursor_color);
///         // etc.
///     }
/// }
/// ```
pub struct DefaultThemeHandler;

impl ThemeHandler for DefaultThemeHandler {
    fn apply_theme(&self, theme: &Theme, settings: &mut UserSettings) {
        // Store theme name
        settings.theme = Some(theme.name.to_string());

        // Apply background and foreground colors
        settings.editor_bg = Some(theme.background);
        settings.editor_fg = Some(theme.foreground);

        // Apply syntax colors
        settings.syntax_colors = theme.syntax.clone();
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
    /// Theme variant (light/dark)
    pub variant: ThemeVariant,
    /// Background color
    pub background: Color,
    /// Foreground (text) color
    pub foreground: Color,
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
        syntax: Option<SyntaxColors>,
    ) -> Self {
        Theme {
            name,
            variant,
            background,
            foreground,
            syntax,
        }
    }

    /// Get the light theme
    #[must_use]
    pub fn light() -> Self {
        use crate::constants::captures::*;
        let syntax = SyntaxColors::from_base_colors(&[
            (KEYWORD, Color::DarkMagenta),
            (FUNCTION, Color::DarkBlue),
            (TYPE, Color::DarkYellow),
            (STRING, Color::DarkGreen),
            (NUMBER, Color::DarkCyan),
            (CONSTANT, Color::DarkCyan),
            (BOOLEAN, Color::DarkCyan),
            (COMMENT, Color::DarkGrey),
            (VARIABLE, Color::Black),
            (PARAMETER, Color::Black),
            (PROPERTY, Color::Black),
            (ATTRIBUTE, Color::Black),
            (NAMESPACE, Color::Black),
            (OPERATOR, Color::Black),
            (PUNCTUATION, Color::Black),
            (CONSTRUCTOR, Color::DarkYellow),
            (BUILTIN, Color::DarkBlue),
            (TEXT_TITLE, Color::DarkBlue),
            (TEXT_LITERAL, Color::DarkGreen),
            (TEXT_REFERENCE, Color::DarkYellow), // Distinct from URI
            (TEXT_URI, Color::DarkCyan),         // Distinct from Reference
            // New mappings from coverage check
            (TAG, Color::DarkBlue),
            (LABEL, Color::DarkYellow),
            (ESCAPE, Color::DarkCyan),
            ("method", Color::DarkBlue),
            ("conditional", Color::DarkMagenta),
            ("repeat", Color::DarkMagenta),
            ("preproc", Color::DarkMagenta),
            ("delimiter", Color::Black),
            ("embedded", Color::Black), // Plain text
            ("none", Color::Black),
            // CSS/At-rules
            ("charset", Color::DarkMagenta),
            ("import", Color::DarkMagenta),
            ("keyframes", Color::DarkMagenta),
            ("media", Color::DarkMagenta),
            ("supports", Color::DarkMagenta),
            ("field", Color::Black), // Property
            (C_IMPORT, Color::DarkMagenta),
            (CHARACTER, Color::DarkGreen),
            (MODULE_BUILTIN, Color::DarkBlue),
            (SPELL, Color::Black),
            (MODULE, Color::DarkYellow),
        ]);

        Theme::new(
            crate::constants::themes::LIGHT,
            ThemeVariant::Light,
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            }, // #FFFFFF - Pure white
            Color::Rgb { r: 0, g: 0, b: 0 }, // #000000 - Pure black
            Some(syntax),
        )
    }

    /// Get the dark theme
    #[must_use]
    pub fn dark() -> Self {
        use crate::constants::captures::*;
        let syntax = SyntaxColors::from_base_colors(&[
            (KEYWORD, Color::Magenta),
            (FUNCTION, Color::Blue),
            (TYPE, Color::Yellow),
            (STRING, Color::Green),
            (NUMBER, Color::Cyan),
            (CONSTANT, Color::Cyan),
            (BOOLEAN, Color::Cyan),
            (COMMENT, Color::Grey),
            (VARIABLE, Color::White),
            (PARAMETER, Color::White),
            (PROPERTY, Color::White),
            (ATTRIBUTE, Color::White),
            (NAMESPACE, Color::White),
            (OPERATOR, Color::White),
            (PUNCTUATION, Color::White),
            (CONSTRUCTOR, Color::Yellow),
            (BUILTIN, Color::Blue),
            (TEXT_TITLE, Color::Blue),
            (TEXT_LITERAL, Color::Green),
            (TEXT_REFERENCE, Color::Yellow), // Distinct from URI
            (TEXT_URI, Color::Cyan),         // Distinct from Reference
            // New mappings from coverage check
            (TAG, Color::Blue),
            (LABEL, Color::Yellow),
            (ESCAPE, Color::Cyan),
            ("method", Color::Blue),
            ("conditional", Color::Magenta),
            ("repeat", Color::Magenta),
            ("preproc", Color::Magenta),
            ("delimiter", Color::White),
            ("embedded", Color::White),
            ("none", Color::White),
            // CSS/At-rules
            ("charset", Color::Magenta),
            ("import", Color::Magenta),
            ("keyframes", Color::Magenta),
            ("media", Color::Magenta),
            ("supports", Color::Magenta),
            ("field", Color::White), // Property
            (C_IMPORT, Color::Magenta),
            (CHARACTER, Color::Green),
            (MODULE_BUILTIN, Color::Blue),
            (SPELL, Color::White),
            (MODULE, Color::Yellow),
        ]);

        Theme::new(
            crate::constants::themes::DARK,
            ThemeVariant::Dark,
            Color::Rgb {
                r: 30,
                g: 30,
                b: 30,
            }, // #1E1E1E - Dark gray
            Color::Rgb {
                r: 224,
                g: 224,
                b: 224,
            }, // #E0E0E0 - Light gray
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
        let syntax = SyntaxColors::from_base_colors(&[
            (
                KEYWORD,
                Color::Rgb {
                    r: 251,
                    g: 73,
                    b: 52,
                },
            ), // #fb4934
            (
                FUNCTION,
                Color::Rgb {
                    r: 238,
                    g: 189,
                    b: 53,
                },
            ), // #eebd35
            (
                TYPE,
                Color::Rgb {
                    r: 142,
                    g: 192,
                    b: 124,
                },
            ), // #8ec07c
            (
                STRING,
                Color::Rgb {
                    r: 152,
                    g: 151,
                    b: 26,
                },
            ), // #98971a
            (
                NUMBER,
                Color::Rgb {
                    r: 177,
                    g: 98,
                    b: 134,
                },
            ), // #b16286
            (
                CONSTANT,
                Color::Rgb {
                    r: 212,
                    g: 135,
                    b: 156,
                },
            ), // #D4879C
            (
                BOOLEAN,
                Color::Rgb {
                    r: 214,
                    g: 93,
                    b: 14,
                },
            ), // #d65d0e
            (
                COMMENT,
                Color::Rgb {
                    r: 102,
                    g: 92,
                    b: 84,
                },
            ), // #665c54
            (
                VARIABLE,
                Color::Rgb {
                    r: 127,
                    g: 162,
                    b: 172,
                },
            ), // #7fa2ac
            (
                PARAMETER,
                Color::Rgb {
                    r: 69,
                    g: 133,
                    b: 136,
                },
            ), // #458588
            (
                PROPERTY,
                Color::Rgb {
                    r: 69,
                    g: 133,
                    b: 136,
                },
            ), // #458588
            (
                ATTRIBUTE,
                Color::Rgb {
                    r: 69,
                    g: 133,
                    b: 136,
                },
            ), // #458588
            (
                NAMESPACE,
                Color::Rgb {
                    r: 127,
                    g: 162,
                    b: 172,
                },
            ), // #7fa2ac
            (OPERATOR, fg),
            (PUNCTUATION, fg),
            (
                CONSTRUCTOR,
                Color::Rgb {
                    r: 142,
                    g: 192,
                    b: 124,
                },
            ), // #8ec07c
            (
                BUILTIN,
                Color::Rgb {
                    r: 69,
                    g: 133,
                    b: 136,
                },
            ), // #458588
            (
                TEXT_TITLE,
                Color::Rgb {
                    r: 238,
                    g: 189,
                    b: 53,
                },
            ), // Yellow
            (
                TEXT_LITERAL,
                Color::Rgb {
                    r: 152,
                    g: 151,
                    b: 26,
                },
            ), // Green
            (
                TEXT_REFERENCE,
                Color::Rgb {
                    r: 250,
                    g: 189,
                    b: 47,
                },
            ), // Yellow #fabd2f
            (
                TEXT_URI,
                Color::Rgb {
                    r: 131,
                    g: 165,
                    b: 152,
                },
            ), // Blue #83a598
            // Extensions
            (
                TAG,
                Color::Rgb {
                    r: 142,
                    g: 192,
                    b: 124,
                },
            ), // Aqua/Green
            (
                LABEL,
                Color::Rgb {
                    r: 250,
                    g: 189,
                    b: 47,
                },
            ), // Yellow
            (
                ESCAPE,
                Color::Rgb {
                    r: 214,
                    g: 93,
                    b: 14,
                },
            ), // Orange
            (
                "method",
                Color::Rgb {
                    r: 238,
                    g: 189,
                    b: 53,
                },
            ), // Yellow
            (
                "conditional",
                Color::Rgb {
                    r: 251,
                    g: 73,
                    b: 52,
                },
            ), // Red
            (
                "repeat",
                Color::Rgb {
                    r: 251,
                    g: 73,
                    b: 52,
                },
            ), // Red
            (
                "preproc",
                Color::Rgb {
                    r: 142,
                    g: 192,
                    b: 124,
                },
            ), // Aqua
            ("delimiter", fg),
            ("embedded", fg),
            ("none", fg),
            (
                "charset",
                Color::Rgb {
                    r: 251,
                    g: 73,
                    b: 52,
                },
            ), // Red
            (
                "import",
                Color::Rgb {
                    r: 142,
                    g: 192,
                    b: 124,
                },
            ), // Aqua
            (
                "keyframes",
                Color::Rgb {
                    r: 251,
                    g: 73,
                    b: 52,
                },
            ), // Red
            (
                "media",
                Color::Rgb {
                    r: 251,
                    g: 73,
                    b: 52,
                },
            ), // Red
            (
                "supports",
                Color::Rgb {
                    r: 251,
                    g: 73,
                    b: 52,
                },
            ), // Red
            (
                "field",
                Color::Rgb {
                    r: 69,
                    g: 133,
                    b: 136,
                },
            ), // Blue
            (
                C_IMPORT,
                Color::Rgb {
                    r: 142,
                    g: 192,
                    b: 124,
                },
            ), // Aqua
            (
                CHARACTER,
                Color::Rgb {
                    r: 152,
                    g: 151,
                    b: 26,
                },
            ), // Green
            (
                MODULE_BUILTIN,
                Color::Rgb {
                    r: 69,
                    g: 133,
                    b: 136,
                },
            ), // Blue
            (SPELL, fg),
            (
                MODULE,
                Color::Rgb {
                    r: 127,
                    g: 162,
                    b: 172,
                },
            ), // Blue
        ]);

        Theme::new(
            crate::constants::themes::GRUVBOX,
            ThemeVariant::Dark,
            Color::Rgb {
                r: 40,
                g: 40,
                b: 32,
            }, // #282828
            fg,
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
        }; // #BBC3D4
        let syntax = SyntaxColors::from_base_colors(&[
            (
                KEYWORD,
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // #B48EAD
            (
                FUNCTION,
                Color::Rgb {
                    r: 136,
                    g: 192,
                    b: 208,
                },
            ), // #88C0D0
            (
                TYPE,
                Color::Rgb {
                    r: 129,
                    g: 161,
                    b: 193,
                },
            ), // #81A1C1
            (
                STRING,
                Color::Rgb {
                    r: 163,
                    g: 190,
                    b: 140,
                },
            ), // #A3BE8C
            (
                NUMBER,
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // #B48EAD
            (
                CONSTANT,
                Color::Rgb {
                    r: 208,
                    g: 135,
                    b: 112,
                },
            ), // #D08770
            (
                BOOLEAN,
                Color::Rgb {
                    r: 208,
                    g: 135,
                    b: 112,
                },
            ), // #D08770
            (
                COMMENT,
                Color::Rgb {
                    r: 76,
                    g: 86,
                    b: 106,
                },
            ), // #4C566A
            (VARIABLE, fg),
            (PARAMETER, fg),
            (
                PROPERTY,
                Color::Rgb {
                    r: 136,
                    g: 192,
                    b: 208,
                },
            ), // #88C0D0
            (
                ATTRIBUTE,
                Color::Rgb {
                    r: 143,
                    g: 188,
                    b: 187,
                },
            ), // #8FBCBB
            (
                NAMESPACE,
                Color::Rgb {
                    r: 129,
                    g: 161,
                    b: 193,
                },
            ), // #81A1C1
            (OPERATOR, fg),
            (PUNCTUATION, fg),
            (
                CONSTRUCTOR,
                Color::Rgb {
                    r: 129,
                    g: 161,
                    b: 193,
                },
            ), // #81A1C1
            (
                BUILTIN,
                Color::Rgb {
                    r: 143,
                    g: 188,
                    b: 187,
                },
            ), // #8FBCBB
            (
                TEXT_TITLE,
                Color::Rgb {
                    r: 136,
                    g: 192,
                    b: 208,
                },
            ), // #88C0D0
            (
                TEXT_LITERAL,
                Color::Rgb {
                    r: 163,
                    g: 190,
                    b: 140,
                },
            ), // #A3BE8C
            (
                TEXT_REFERENCE,
                Color::Rgb {
                    r: 235,
                    g: 203,
                    b: 139,
                },
            ), // Yellow #EBCB8B
            (
                TEXT_URI,
                Color::Rgb {
                    r: 143,
                    g: 188,
                    b: 187,
                },
            ), // Cyan #8FBCBB
            // Extensions
            (
                TAG,
                Color::Rgb {
                    r: 129,
                    g: 161,
                    b: 193,
                },
            ), // Blue
            (
                LABEL,
                Color::Rgb {
                    r: 235,
                    g: 203,
                    b: 139,
                },
            ), // Yellow
            (
                ESCAPE,
                Color::Rgb {
                    r: 208,
                    g: 135,
                    b: 112,
                },
            ), // Orange
            (
                "method",
                Color::Rgb {
                    r: 136,
                    g: 192,
                    b: 208,
                },
            ), // Cyan
            (
                "conditional",
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // Purple
            (
                "repeat",
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // Purple
            (
                "preproc",
                Color::Rgb {
                    r: 129,
                    g: 161,
                    b: 193,
                },
            ), // Blue
            ("delimiter", fg),
            ("embedded", fg),
            ("none", fg),
            (
                "charset",
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // Purple
            (
                "import",
                Color::Rgb {
                    r: 129,
                    g: 161,
                    b: 193,
                },
            ), // Blue
            (
                "keyframes",
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // Purple
            (
                "media",
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // Purple
            (
                "supports",
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // Purple
            (
                C_IMPORT,
                Color::Rgb {
                    r: 180,
                    g: 142,
                    b: 173,
                },
            ), // Purple
            (
                CHARACTER,
                Color::Rgb {
                    r: 163,
                    g: 190,
                    b: 140,
                },
            ), // Green
            (
                MODULE_BUILTIN,
                Color::Rgb {
                    r: 143,
                    g: 188,
                    b: 187,
                },
            ), // Cyan/Teal
            (SPELL, fg),
            (
                MODULE,
                Color::Rgb {
                    r: 129,
                    g: 161,
                    b: 193,
                },
            ), // Blue
        ]);

        Theme::new(
            crate::constants::themes::NORDIC,
            ThemeVariant::Dark,
            Color::Rgb {
                r: 46,
                g: 52,
                b: 64,
            }, // #2E3440
            fg,
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
