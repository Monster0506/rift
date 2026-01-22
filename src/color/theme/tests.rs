use super::*;
use crate::state::UserSettings;

#[test]
fn test_theme_creation() {
    let light = Theme::light();
    assert_eq!(light.name, "light");
    assert_eq!(light.variant, ThemeVariant::Light);

    let dark = Theme::dark();
    assert_eq!(dark.name, "dark");
    assert_eq!(dark.variant, ThemeVariant::Dark);
}

#[test]
fn test_theme_by_name() {
    assert!(Theme::by_name("light").is_some());
    assert!(Theme::by_name("dark").is_some());
    assert!(Theme::by_name("gruvbox").is_some());
    assert!(Theme::by_name("nordic").is_some());
    assert!(Theme::by_name("unknown").is_none());
}

#[test]
fn test_theme_by_name_case_insensitive() {
    assert!(Theme::by_name("LIGHT").is_some());
    assert!(Theme::by_name("Dark").is_some());
    assert!(Theme::by_name("GRUVBOX").is_some());
    assert!(Theme::by_name("Nordic").is_some());
}

#[test]
fn test_available_themes() {
    let themes = Theme::available_themes();
    assert!(themes.contains(&"light"));
    assert!(themes.contains(&"dark"));
    assert!(themes.contains(&"gruvbox"));
    assert!(themes.contains(&"nordic"));
    assert_eq!(themes.len(), 4);
}

#[test]
fn test_theme_colors() {
    let light = Theme::light();
    match light.background {
        Color::Rgb { r, g, b } => {
            assert_eq!(r, 255);
            assert_eq!(g, 255);
            assert_eq!(b, 255);
        }
        _ => panic!("Light theme should have RGB background"),
    }
    match light.foreground {
        Color::Rgb { r, g, b } => {
            assert_eq!(r, 0);
            assert_eq!(g, 0);
            assert_eq!(b, 0);
        }
        _ => panic!("Light theme should have RGB foreground"),
    }
}

#[test]
fn test_theme_handler_applies_colors() {
    let handler = DefaultThemeHandler;
    let mut settings = UserSettings::new();
    let theme = Theme::dark();

    handler.apply_theme(&theme, &mut settings);

    assert_eq!(settings.theme, Some("dark".to_string()));
    assert_eq!(settings.editor_bg, Some(theme.background));
    assert_eq!(settings.editor_fg, Some(theme.foreground));
}

#[test]
fn test_theme_handler_applies_gruvbox() {
    let handler = DefaultThemeHandler;
    let mut settings = UserSettings::new();
    let theme = Theme::gruvbox();

    handler.apply_theme(&theme, &mut settings);

    assert_eq!(settings.theme, Some("gruvbox".to_string()));
    assert_eq!(settings.editor_bg, Some(theme.background));
    assert_eq!(settings.editor_fg, Some(theme.foreground));
}

#[test]
fn test_theme_handler_applies_nordic() {
    let handler = DefaultThemeHandler;
    let mut settings = UserSettings::new();
    let theme = Theme::nordic();

    handler.apply_theme(&theme, &mut settings);

    assert_eq!(settings.theme, Some("nordic".to_string()));
    assert_eq!(settings.editor_bg, Some(theme.background));
    assert_eq!(settings.editor_fg, Some(theme.foreground));
}

#[test]
fn test_theme_handler_overwrites_existing_theme() {
    let handler = DefaultThemeHandler;
    let mut settings = UserSettings::new();

    // Apply light theme first
    let light = Theme::light();
    handler.apply_theme(&light, &mut settings);
    assert_eq!(settings.theme, Some("light".to_string()));
    assert_eq!(settings.editor_bg, Some(light.background));

    // Apply dark theme - should overwrite
    let dark = Theme::dark();
    handler.apply_theme(&dark, &mut settings);
    assert_eq!(settings.theme, Some("dark".to_string()));
    assert_eq!(settings.editor_bg, Some(dark.background));
    assert_eq!(settings.editor_fg, Some(dark.foreground));
}

#[test]
fn test_theme_apply_to_settings() {
    let mut settings = UserSettings::new();
    let theme = Theme::light();

    theme.apply_to_settings(&mut settings);

    assert_eq!(settings.theme, Some("light".to_string()));
    assert_eq!(settings.editor_bg, Some(theme.background));
    assert_eq!(settings.editor_fg, Some(theme.foreground));
}

#[test]
fn test_all_themes_have_valid_colors() {
    let themes = vec![
        Theme::light(),
        Theme::dark(),
        Theme::gruvbox(),
        Theme::nordic(),
    ];

    for theme in themes {
        // Verify background is not Reset
        assert_ne!(theme.background, Color::Reset);
        // Verify foreground is not Reset
        assert_ne!(theme.foreground, Color::Reset);
        // Verify background and foreground are different
        assert_ne!(theme.background, theme.foreground);
    }
}

#[test]
fn test_theme_variant() {
    let light = Theme::light();
    assert_eq!(light.variant, ThemeVariant::Light);

    let dark = Theme::dark();
    assert_eq!(dark.variant, ThemeVariant::Dark);

    let gruvbox = Theme::gruvbox();
    assert_eq!(gruvbox.variant, ThemeVariant::Dark);

    let nordic = Theme::nordic();
    assert_eq!(nordic.variant, ThemeVariant::Dark);
}

#[test]
#[cfg(feature = "treesitter")]
fn test_theme_coverage() {
    use crate::syntax::loader::LanguageLoader;
    use std::path::PathBuf;

    let languages = vec![
        "rust",
        "python",
        "c",
        "cpp",
        "javascript",
        "typescript",
        "go",
        "lua",
        "json",
        "bash",
        "markdown",
        "yaml",
        "html",
        "css",
        "java",
        "c_sharp",
        "ruby",
        "php",
        "zig",
    ];

    let theme = Theme::dark();
    let syntax = theme
        .syntax
        .as_ref()
        .expect("Dark theme must have syntax colors");
    let loader = LanguageLoader::new(PathBuf::from(""));

    let mut missing_captures = Vec::new();

    for lang in languages {
        // Load highlights query
        let query_source = match loader.load_query(lang, "highlights") {
            Ok(q) => q,
            Err(_) => {
                continue;
            }
        };

        // Simple parser to extract @capture.name
        let mut chars = query_source.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            if c == '@' {
                // Found potential capture
                let start = i;
                let mut end = i + 1;
                while let Some(&(j, ch)) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
                        chars.next();
                        end = j + 1;
                    } else {
                        break;
                    }
                }

                let capture = &query_source[start..end];
                // capture includes leading @
                if capture.trim_start_matches('@').trim().is_empty() {
                    continue;
                }

                // Verify if theme handles it
                if syntax.get_color(capture).is_none() {
                    missing_captures.push(format!("{} in {}", capture, lang));
                }
            }
        }
    }

    if !missing_captures.is_empty() {
        // Deduplicate
        missing_captures.sort();
        missing_captures.dedup();

        // Write to file
        use std::io::Write;
        let mut file = std::fs::File::create("missing_captures.txt").unwrap();
        for capture in &missing_captures {
            writeln!(file, "{}", capture).unwrap();
        }

        panic!(
            "Found {} missing captures. See missing_captures.txt",
            missing_captures.len()
        );
    }
}
