use super::*;

#[cfg(windows)]
const SYSTEM_LIB: &str = "kernel32.dll";
#[cfg(all(unix, not(target_os = "macos")))]
const SYSTEM_LIB: &str = "libc.so.6";
#[cfg(target_os = "macos")]
const SYSTEM_LIB: &str = "libSystem.B.dylib";

const MISSING_SYMBOL: &str = "tree_sitter_not_a_language";

#[test]
fn missing_symbol_is_reported() {
    let loader = LanguageLoader::new(PathBuf::new());

    let err = loader
        .register_grammar("fake", SYSTEM_LIB, MISSING_SYMBOL)
        .unwrap_err();

    assert!(err.contains(MISSING_SYMBOL), "{err}");
    assert!(loader.load_language("fake").is_err());
}

#[test]
fn every_listed_bundled_language_loads_with_a_highlights_query() {
    let loader = LanguageLoader::new(PathBuf::new());
    for name in BUNDLED_LANGUAGES {
        assert!(loader.load_language(name).is_ok(), "{name} failed to load");
        assert!(
            loader.query_source(name, QueryKind::Highlights).is_some(),
            "{name} has no highlights query"
        );
    }
}

#[test]
fn clearing_a_query_override_restores_the_bundled_query() {
    let loader = LanguageLoader::new(PathBuf::new());
    let bundled = loader.query_source("lua", QueryKind::Highlights);
    loader.set_query_override(
        "lua",
        QueryKind::Highlights,
        Some("(comment) @comment".into()),
    );
    assert_eq!(
        loader.query_source("lua", QueryKind::Highlights).as_deref(),
        Some("(comment) @comment")
    );
    loader.set_query_override("lua", QueryKind::Highlights, None);
    assert_eq!(loader.query_source("lua", QueryKind::Highlights), bundled);
}
