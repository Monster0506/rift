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
