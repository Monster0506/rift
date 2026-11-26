use super::*;

#[test]
fn test_theme_creation() {
    let light = Theme::light();
    assert_eq!(light.name, "light");

    let dark = Theme::dark();
    assert_eq!(dark.name, "dark");
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
    for &name in Theme::NAMES {
        assert_eq!(Theme::by_name(name).unwrap().name(), name);
    }
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
#[cfg(feature = "treesitter")]
fn test_theme_coverage() {
    use crate::syntax::loader::LanguageLoader;
    use std::io::Write;
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
        let Ok(query_source) = loader.load_query(lang, "highlights") else {
            continue;
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
                        end = j + ch.len_utf8();
                    } else {
                        break;
                    }
                }

                let capture = query_source.get(start..end).expect("char boundary");
                // capture includes leading @
                if capture.trim_start_matches('@').trim().is_empty() {
                    continue;
                }

                // Verify if theme handles it
                if syntax.get_color(capture).is_none() {
                    missing_captures.push(format!("{capture} in {lang}"));
                }
            }
        }
    }

    if !missing_captures.is_empty() {
        // Deduplicate
        missing_captures.sort();
        missing_captures.dedup();

        // Write to file
        let mut file = std::fs::File::create("missing_captures.txt").unwrap();
        for capture in &missing_captures {
            writeln!(file, "{capture}").unwrap();
        }

        panic!(
            "Found {} missing captures. See missing_captures.txt",
            missing_captures.len()
        );
    }
}
