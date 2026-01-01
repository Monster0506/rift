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
        settings.syntax_colors = theme.syntax;
    }
}

/// Global theme handler instance
/// In the future, this could be made configurable or passed as a parameter
static THEME_HANDLER: DefaultThemeHandler = DefaultThemeHandler;

/// Syntax highlighting colors for a theme
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxColors {
    pub keyword: Color,
    pub function: Color,
    pub type_def: Color,
    pub string: Color,
    pub number: Color,
    pub constant: Color,
    pub boolean: Color,
    pub comment: Color,
    pub variable: Color,
    pub parameter: Color,
    pub property: Color,
    pub attribute: Color,
    pub namespace: Color,
    pub operator: Color,
    pub punctuation: Color,
    pub constructor: Color,
    pub builtin: Color,
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
        Theme::new(
            "light",
            ThemeVariant::Light,
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            }, // #FFFFFF - Pure white
            Color::Rgb { r: 0, g: 0, b: 0 }, // #000000 - Pure black
            None,                            // Default syntax for now
        )
    }

    /// Get the dark theme
    #[must_use]
    pub fn dark() -> Self {
        Theme::new(
            "dark",
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
            None,
        )
    }

    /// Get the gruvbox theme (dark variant)
    #[must_use]
    pub fn gruvbox() -> Self {
        let fg = Color::Rgb {
            r: 235,
            g: 219,
            b: 178,
        };
        let syntax = SyntaxColors {
            keyword: Color::Rgb {
                r: 251,
                g: 73,
                b: 52,
            }, // #fb4934
            function: Color::Rgb {
                r: 238,
                g: 189,
                b: 53,
            }, // #eebd35
            type_def: Color::Rgb {
                r: 142,
                g: 192,
                b: 124,
            }, // #8ec07c
            string: Color::Rgb {
                r: 152,
                g: 151,
                b: 26,
            }, // #98971a
            number: Color::Rgb {
                r: 177,
                g: 98,
                b: 134,
            }, // #b16286
            constant: Color::Rgb {
                r: 212,
                g: 135,
                b: 156,
            }, // #D4879C
            boolean: Color::Rgb {
                r: 214,
                g: 93,
                b: 14,
            }, // #d65d0e
            comment: Color::Rgb {
                r: 102,
                g: 92,
                b: 84,
            }, // #665c54
            variable: Color::Rgb {
                r: 127,
                g: 162,
                b: 172,
            }, // #7fa2ac
            parameter: Color::Rgb {
                r: 69,
                g: 133,
                b: 136,
            }, // #458588
            property: Color::Rgb {
                r: 69,
                g: 133,
                b: 136,
            }, // #458588
            attribute: Color::Rgb {
                r: 69,
                g: 133,
                b: 136,
            }, // #458588
            namespace: Color::Rgb {
                r: 127,
                g: 162,
                b: 172,
            }, // #7fa2ac
            operator: fg,
            punctuation: fg,
            constructor: Color::Rgb {
                r: 142,
                g: 192,
                b: 124,
            }, // #8ec07c
            builtin: Color::Rgb {
                r: 69,
                g: 133,
                b: 136,
            }, // #458588
        };

        Theme::new(
            "gruvbox",
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

    /// Get the nordic theme (Nord)
    #[must_use]
    pub fn nordic() -> Self {
        let fg = Color::Rgb {
            r: 187,
            g: 195,
            b: 212,
        }; // #BBC3D4
        let syntax = SyntaxColors {
            keyword: Color::Rgb {
                r: 180,
                g: 142,
                b: 173,
            }, // #B48EAD
            function: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            }, // #88C0D0
            type_def: Color::Rgb {
                r: 129,
                g: 161,
                b: 193,
            }, // #81A1C1
            string: Color::Rgb {
                r: 163,
                g: 190,
                b: 140,
            }, // #A3BE8C
            number: Color::Rgb {
                r: 180,
                g: 142,
                b: 173,
            }, // #B48EAD
            constant: Color::Rgb {
                r: 208,
                g: 135,
                b: 112,
            }, // #D08770
            boolean: Color::Rgb {
                r: 208,
                g: 135,
                b: 112,
            }, // #D08770
            comment: Color::Rgb {
                r: 76,
                g: 86,
                b: 106,
            }, // #4C566A
            variable: fg,
            parameter: fg,
            property: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            }, // #88C0D0
            attribute: Color::Rgb {
                r: 143,
                g: 188,
                b: 187,
            }, // #8FBCBB
            namespace: Color::Rgb {
                r: 129,
                g: 161,
                b: 193,
            }, // #81A1C1
            operator: fg,
            punctuation: fg,
            constructor: Color::Rgb {
                r: 129,
                g: 161,
                b: 193,
            }, // #81A1C1
            builtin: Color::Rgb {
                r: 143,
                g: 188,
                b: 187,
            }, // #8FBCBB
        };

        Theme::new(
            "nordic",
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
            "light" => Some(Theme::light()),
            "dark" => Some(Theme::dark()),
            "gruvbox" => Some(Theme::gruvbox()),
            "nordic" | "nord" => Some(Theme::nordic()),
            _ => None,
        }
    }

    /// Get all available theme names
    #[must_use]
    pub fn available_themes() -> Vec<&'static str> {
        vec!["light", "dark", "gruvbox", "nordic"]
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
