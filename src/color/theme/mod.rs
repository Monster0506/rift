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
        
        // Future: When Theme struct is extended with more properties,
        // add them here. For example:
        // settings.status_bar_bg = Some(theme.status_bar_bg);
        // settings.status_bar_fg = Some(theme.status_bar_fg);
        // settings.selection_bg = Some(theme.selection_bg);
        // settings.cursor_color = Some(theme.cursor_color);
        // settings.command_line_bg = Some(theme.command_line_bg);
        // etc.
    }
}

/// Global theme handler instance
/// In the future, this could be made configurable or passed as a parameter
static THEME_HANDLER: DefaultThemeHandler = DefaultThemeHandler;

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
}

/// Theme variant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeVariant {
    Light,
    Dark,
}

impl Theme {
    /// Create a new theme
    pub fn new(name: &'static str, variant: ThemeVariant, background: Color, foreground: Color) -> Self {
        Theme {
            name,
            variant,
            background,
            foreground,
        }
    }

    /// Get the light theme
    pub fn light() -> Self {
        Theme::new(
            "light",
            ThemeVariant::Light,
            Color::Rgb { r: 255, g: 255, b: 255 }, // #FFFFFF - Pure white
            Color::Rgb { r: 0, g: 0, b: 0 },       // #000000 - Pure black
        )
    }

    /// Get the dark theme
    pub fn dark() -> Self {
        Theme::new(
            "dark",
            ThemeVariant::Dark,
            Color::Rgb { r: 30, g: 30, b: 30 },    // #1E1E1E - Dark gray
            Color::Rgb { r: 224, g: 224, b: 224 }, // #E0E0E0 - Light gray
        )
    }

    /// Get the gruvbox theme (dark variant)
    pub fn gruvbox() -> Self {
        Theme::new(
            "gruvbox",
            ThemeVariant::Dark,
            Color::Rgb { r: 40, g: 40, b: 32 },    // #282828 - Gruvbox dark background
            Color::Rgb { r: 235, g: 219, b: 178 }, // #EBDBB2 - Gruvbox beige foreground
        )
    }

    /// Get the nordic theme (Nord)
    pub fn nordic() -> Self {
        Theme::new(
            "nordic",
            ThemeVariant::Dark,
            Color::Rgb { r: 46, g: 52, b: 64 },    // #2E3440 - Nord polar night
            Color::Rgb { r: 216, g: 222, b: 233 }, // #D8DEE9 - Nord snow storm
        )
    }

    /// Get theme by name
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
