use crate::error::{ErrorType, RiftError};
use std::collections::HashMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};
#[cfg(feature = "treesitter")]
use std::sync::Mutex;
use std::sync::{Arc, RwLock};

/// The loaded-grammar handle type; a real tree-sitter `Language` when the
/// feature is on, or an inert stand-in when it's compiled out entirely.
#[cfg(feature = "treesitter")]
pub type Language = tree_sitter::Language;
#[cfg(not(feature = "treesitter"))]
#[derive(Clone)]
pub struct Language;

/// Handle to a loaded language, keeping the backing dynamic library alive.
/// `lib` must be cloned alongside every clone of `language` or the memory it points into can be unmapped.
#[derive(Clone)]
pub struct LoadedLanguage {
    pub language: Language,
    pub name: String,
    pub lib: Option<Arc<RawLib>>,
}

impl LoadedLanguage {
    /// Create a LoadedLanguage for bundled grammars (no library handle needed)
    #[allow(dead_code)]
    pub fn bundled(language: Language, name: &str) -> Self {
        Self {
            language,
            name: name.to_string(),
            lib: None,
        }
    }
}

/// Runtime-registered language customisations from plugins.
#[derive(Default)]
pub struct DynamicRegistry {
    /// File extension -> language name overrides (e.g. "svelte" -> "svelte").
    pub filetype_map: HashMap<String, String>,
    /// Language name -> highlights query source override.
    pub highlights_overrides: HashMap<String, String>,
    /// Language name -> injections query source.
    pub injections_queries: HashMap<String, String>,
}

pub struct LanguageLoader {
    _grammar_dir: PathBuf,
    pub dynamic: RwLock<DynamicRegistry>,
    /// Shared libraries loaded at runtime, kept alive only via the `Arc<RawLib>`
    /// bundled into each `dynamic_languages` entry; never removed once inserted.
    #[cfg(feature = "treesitter")]
    loaded_libs: Mutex<Vec<Arc<RawLib>>>,
    /// Grammars registered via `register_grammar()`, keyed by language name.
    dynamic_languages: RwLock<HashMap<String, LoadedLanguage>>,
}

impl LanguageLoader {
    pub fn new(grammar_dir: PathBuf) -> Self {
        Self {
            _grammar_dir: grammar_dir,
            dynamic: RwLock::new(DynamicRegistry::default()),
            #[cfg(feature = "treesitter")]
            loaded_libs: Mutex::new(Vec::new()),
            dynamic_languages: RwLock::new(HashMap::new()),
        }
    }

    // -------------------------------------------------------------------------
    // Dynamic registration (called from plugin mutations)
    // -------------------------------------------------------------------------

    pub fn register_filetype(&self, ext: &str, lang_name: &str) {
        let mut reg = self.dynamic.write().unwrap_or_else(|e| e.into_inner());
        reg.filetype_map
            .insert(ext.to_string(), lang_name.to_string());
    }

    /// Return the language name for a file path from the filetype registry alone,
    /// without requiring a tree-sitter grammar to be available. Returns `None` for
    /// files with no extension or an unrecognised extension.
    pub fn language_name_for_file(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        // Dynamic registry (from plugins) takes priority.
        {
            let reg = self.dynamic.read().unwrap_or_else(|e| e.into_inner());
            if let Some(name) = reg.filetype_map.get(ext) {
                return Some(name.clone());
            }
        }
        // Built-in extension map (mirrors load_language_for_file).
        let name = match ext {
            "c" | "h" => "c",
            "cc" | "cpp" | "cxx" | "hpp" => "cpp",
            "css" => "css",
            "go" => "go",
            "html" | "htm" => "html",
            "js" | "jsx" | "mjs" => "javascript",
            "json" => "json",
            "lua" => "lua",
            "md" | "markdown" => "markdown",
            "py" => "python",
            "rs" => "rust",
            "sh" | "bash" | "zsh" => "bash",
            "svelte" => "svelte",
            "ts" => "typescript",
            "tsx" => "tsx",
            "yaml" | "yml" => "yaml",
            "java" => "java",
            "cs" => "c_sharp",
            "rb" => "ruby",
            "php" => "php",
            "zig" => "zig",
            "sql" => "sql",
            _ => return None,
        };
        Some(name.to_string())
    }

    pub fn register_language_query(&self, lang_name: &str, query_src: &str) {
        let mut reg = self.dynamic.write().unwrap_or_else(|e| e.into_inner());
        reg.highlights_overrides
            .insert(lang_name.to_string(), query_src.to_string());
    }

    pub fn register_injections_query(&self, lang_name: &str, query_src: &str) {
        let mut reg = self.dynamic.write().unwrap_or_else(|e| e.into_inner());
        reg.injections_queries
            .insert(lang_name.to_string(), query_src.to_string());
    }

    /// Load a tree-sitter grammar from a shared library at runtime.
    ///
    /// `so_path`: path to the compiled `.so` / `.dll` / `.dylib`.
    /// `fn_name`: exported C symbol, e.g. `"tree_sitter_toml"`.
    ///
    /// The library is kept alive for the lifetime of this `LanguageLoader`.
    /// After a successful call, `lang_name` can be used with `register_filetype`
    /// and `register_language_query` from Lua like any built-in language.
    #[cfg(feature = "treesitter")]
    pub fn register_grammar(
        &self,
        lang_name: &str,
        so_path: &str,
        fn_name: &str,
    ) -> Result<(), String> {
        // Re-registering an already-loaded grammar is a no-op: avoids leaking
        // a second library handle and shadowing the existing Language entry.
        if self
            .dynamic_languages
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(lang_name)
        {
            return Ok(());
        }

        let lib =
            unsafe { RawLib::open(so_path).map_err(|e| format!("dlopen '{}': {}", so_path, e))? };

        // SAFETY: `language` borrows code/data inside `lib`; the `Arc<RawLib>`
        // below must be cloned into every copy of `language` that escapes this loader.
        let language: Language = unsafe {
            type LangFn = unsafe extern "C" fn() -> *const tree_sitter::ffi::TSLanguage;
            let sym = lib
                .sym(fn_name.as_bytes())
                .map_err(|e| format!("symbol '{}' in '{}': {}", fn_name, so_path, e))?;
            let f: LangFn = std::mem::transmute(sym);
            let ptr = f();
            if ptr.is_null() {
                return Err(format!("'{}' returned a null TSLanguage pointer", fn_name));
            }
            Language::from_raw(ptr)
        };

        let lib = Arc::new(lib);

        self.loaded_libs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(lib.clone());

        self.dynamic_languages
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                lang_name.to_string(),
                LoadedLanguage {
                    language,
                    name: lang_name.to_string(),
                    lib: Some(lib),
                },
            );

        Ok(())
    }

    /// No-op when tree-sitter is compiled out: there is no grammar type to load into.
    #[cfg(not(feature = "treesitter"))]
    pub fn register_grammar(
        &self,
        _lang_name: &str,
        _so_path: &str,
        _fn_name: &str,
    ) -> Result<(), String> {
        Err("tree-sitter support is not compiled in".to_string())
    }

    /// Test-only entry point exercising the same dedup check as `register_grammar`,
    /// without requiring a caller-supplied dynamic library on disk.
    #[cfg(all(test, feature = "treesitter"))]
    pub(crate) fn register_grammar_for_test(&self, lang_name: &str, language: Language) -> bool {
        if self
            .dynamic_languages
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(lang_name)
        {
            return false;
        }
        let lib = Arc::new(unsafe { RawLib::open(test_lib_path()).expect("open test library") });
        self.loaded_libs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(lib.clone());
        self.dynamic_languages
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                lang_name.to_string(),
                LoadedLanguage {
                    language,
                    name: lang_name.to_string(),
                    lib: Some(lib),
                },
            );
        true
    }

    #[cfg(all(test, feature = "treesitter"))]
    pub(crate) fn loaded_libs_count(&self) -> usize {
        self.loaded_libs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    // -------------------------------------------------------------------------
    // Language loading
    // -------------------------------------------------------------------------

    /// Load a language based on file extension, checking the dynamic registry first.
    pub fn load_language_for_file(&self, path: &Path) -> Result<LoadedLanguage, RiftError> {
        let extension = path.extension().and_then(|e| e.to_str()).ok_or_else(|| {
            RiftError::new(ErrorType::Internal, "NO_EXTENSION", "File has no extension")
        })?;

        // Dynamic registry takes priority.
        let dyn_lang = {
            let reg = self.dynamic.read().unwrap_or_else(|e| e.into_inner());
            reg.filetype_map.get(extension).cloned()
        };

        let lang_name: &str = if let Some(ref dl) = dyn_lang {
            dl.as_str()
        } else {
            match extension {
                "c" | "h" => "c",
                "cc" | "cpp" | "cxx" | "hpp" => "cpp",
                "css" => "css",
                "go" => "go",
                "html" | "htm" => "html",
                "js" | "jsx" | "mjs" => "javascript",
                "json" => "json",
                "lua" => "lua",
                "md" | "markdown" => "markdown",
                "py" => "python",
                "rs" => "rust",
                "sh" | "bash" | "zsh" => "bash",
                "svelte" => "svelte",
                "ts" => "typescript",
                "tsx" => "tsx",
                "yaml" | "yml" => "yaml",
                "java" => "java",
                "cs" => "c_sharp",
                "rb" => "ruby",
                "php" => "php",
                "zig" => "zig",
                "sql" => "sql",
                _ => {
                    return Err(RiftError::new(
                        ErrorType::Internal,
                        "UNKNOWN_EXTENSION",
                        format!("Unknown extension: {}", extension),
                    ))
                }
            }
        };

        self.load_language(lang_name)
    }

    /// Load a specific language by name (e.g., "rust").
    #[allow(unused_variables)]
    pub fn load_language(&self, lang_name: &str) -> Result<LoadedLanguage, RiftError> {
        {
            let langs = self
                .dynamic_languages
                .read()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(loaded) = langs.get(lang_name).cloned() {
                return Ok(loaded);
            }
        }

        #[cfg(feature = "treesitter")]
        if let Some((lang, _)) = get_bundled_language(lang_name) {
            return Ok(LoadedLanguage::bundled(lang, lang_name));
        }

        Err(RiftError::new(
            ErrorType::Internal,
            "LANGUAGE_NOT_FOUND",
            format!("Language {} not found or feature not enabled", lang_name),
        ))
    }

    // -------------------------------------------------------------------------
    // Query loading
    // -------------------------------------------------------------------------

    /// Load a highlights query, checking dynamic overrides first.
    #[allow(unused_variables)]
    pub fn load_query(&self, lang_name: &str, query_name: &str) -> Result<String, RiftError> {
        // Check highlights overrides from plugins.
        if query_name == "highlights" {
            let reg = self.dynamic.read().unwrap_or_else(|e| e.into_inner());
            if let Some(q) = reg.highlights_overrides.get(lang_name) {
                return Ok(q.clone());
            }
        }

        // Bundled queries.
        if query_name == "highlights" {
            #[cfg(feature = "treesitter")]
            if lang_name == "cpp" {
                return Ok(format!(
                    "{}\n{}",
                    tree_sitter_c::HIGHLIGHT_QUERY,
                    tree_sitter_cpp::HIGHLIGHT_QUERY
                ));
            }

            #[cfg(feature = "treesitter")]
            if lang_name == "typescript" || lang_name == "tsx" {
                return Ok(format!(
                    "{}\n{}",
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_typescript::HIGHLIGHTS_QUERY
                ));
            }

            #[cfg(feature = "treesitter")]
            if let Some((_, query)) = get_bundled_language(lang_name) {
                if !query.is_empty() {
                    return Ok(query.to_string());
                }
            }
        }

        Err(RiftError::new(
            ErrorType::Io,
            "QUERY_NOT_FOUND",
            format!(
                "Query {} for {} not found (bundled only)",
                query_name, lang_name
            ),
        ))
    }

    /// Load an injections query for a language.
    /// Checks the dynamic plugin registry first, then bundled queries.
    pub fn load_injections_query(&self, lang_name: &str) -> Option<String> {
        // Plugin-registered injections take priority.
        {
            let reg = self.dynamic.read().unwrap_or_else(|e| e.into_inner());
            if let Some(q) = reg.injections_queries.get(lang_name) {
                return Some(q.clone());
            }
        }

        // Bundled injections.
        get_bundled_injections_query(lang_name).map(str::to_string)
    }
}

// ---------------------------------------------------------------------------
// Bundled grammar tables
// ---------------------------------------------------------------------------

#[cfg(feature = "treesitter")]
fn get_bundled_language(lang_name: &str) -> Option<(Language, &'static str)> {
    match lang_name {
        "rust" => Some((
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
        )),
        "python" => Some((
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
        )),
        "c" => Some((
            tree_sitter_c::LANGUAGE.into(),
            tree_sitter_c::HIGHLIGHT_QUERY,
        )),
        "cpp" => Some((
            tree_sitter_cpp::LANGUAGE.into(),
            tree_sitter_cpp::HIGHLIGHT_QUERY,
        )),
        "javascript" => Some((
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::HIGHLIGHT_QUERY,
        )),
        "typescript" => Some((
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        )),
        "tsx" => Some((
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        )),
        "go" => Some((
            tree_sitter_go::LANGUAGE.into(),
            tree_sitter_go::HIGHLIGHTS_QUERY,
        )),
        "html" => Some((
            tree_sitter_html::LANGUAGE.into(),
            tree_sitter_html::HIGHLIGHTS_QUERY,
        )),
        "css" => Some((
            tree_sitter_css::LANGUAGE.into(),
            tree_sitter_css::HIGHLIGHTS_QUERY,
        )),
        "json" => Some((
            tree_sitter_json::LANGUAGE.into(),
            tree_sitter_json::HIGHLIGHTS_QUERY,
        )),
        "lua" => Some((
            tree_sitter_lua::LANGUAGE.into(),
            tree_sitter_lua::HIGHLIGHTS_QUERY,
        )),
        "markdown" => Some((
            tree_sitter_md::LANGUAGE.into(),
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
        )),
        "yaml" => Some((
            tree_sitter_yaml::LANGUAGE.into(),
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
        )),
        "bash" => Some((
            tree_sitter_bash::LANGUAGE.into(),
            tree_sitter_bash::HIGHLIGHT_QUERY,
        )),
        "java" => Some((
            tree_sitter_java::LANGUAGE.into(),
            tree_sitter_java::HIGHLIGHTS_QUERY,
        )),
        "c_sharp" => Some((tree_sitter_c_sharp::LANGUAGE.into(), "")),
        "ruby" => Some((
            tree_sitter_ruby::LANGUAGE.into(),
            tree_sitter_ruby::HIGHLIGHTS_QUERY,
        )),
        "php" => Some((
            tree_sitter_php::LANGUAGE_PHP.into(),
            tree_sitter_php::HIGHLIGHTS_QUERY,
        )),
        "zig" => Some((
            tree_sitter_zig::LANGUAGE.into(),
            tree_sitter_zig::HIGHLIGHTS_QUERY,
        )),
        "sql" => Some((
            tree_sitter_sequel::LANGUAGE.into(),
            tree_sitter_sequel::HIGHLIGHTS_QUERY,
        )),
        "svelte" => Some((
            tree_sitter_svelte_next::LANGUAGE.into(),
            tree_sitter_svelte_next::HIGHLIGHTS_QUERY,
        )),
        _ => None,
    }
}

/// Built-in injections queries for bundled grammars.
///
/// The capture name in these queries is the target language name.
/// Our simplified injection protocol maps capture name -> language,
/// avoiding the need to parse `#set! injection.language` predicates.
fn get_bundled_injections_query(lang_name: &str) -> Option<&'static str> {
    match lang_name {
        "svelte" => Some(
            // Svelte embeds TypeScript in <script> and CSS in <style>.
            "(script_element (raw_text) @typescript)\n\
             (style_element  (raw_text) @css)",
        ),
        "html" => Some(
            "(script_element (raw_text) @javascript)\n\
             (style_element  (raw_text) @css)",
        ),
        #[cfg(feature = "treesitter")]
        "markdown" => Some(tree_sitter_md::INJECTION_QUERY_BLOCK),
        _ => None,
    }
}

/// A library path guaranteed loadable on this platform, for test-only use.
#[cfg(all(test, feature = "treesitter"))]
fn test_lib_path() -> &'static str {
    #[cfg(windows)]
    {
        "kernel32.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "/usr/lib/libSystem.dylib"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "libc.so.6"
    }
    #[cfg(not(any(unix, windows)))]
    {
        "unused"
    }
}

// Raw dynamic library handle

/// Owns a dlopen/LoadLibrary handle. Any `Language` derived from this library
/// is only valid as long as an `Arc<RawLib>` for it is still reachable.
pub struct RawLib(*mut std::ffi::c_void);

unsafe impl Send for RawLib {}
unsafe impl Sync for RawLib {}

impl RawLib {
    #[cfg_attr(not(any(test, feature = "treesitter")), allow(dead_code))]
    unsafe fn open(path: &str) -> Result<Self, String> {
        let c = CString::new(path).map_err(|e| e.to_string())?;
        let h = sys::open(c.as_ptr());
        if h.is_null() {
            Err(format!("could not open '{}'", path))
        } else {
            Ok(Self(h))
        }
    }

    #[cfg_attr(not(feature = "treesitter"), allow(dead_code))]
    unsafe fn sym(&self, name: &[u8]) -> Result<*mut std::ffi::c_void, String> {
        let c = CString::new(name).map_err(|e| e.to_string())?;
        let s = sys::sym(self.0, c.as_ptr());
        if s.is_null() {
            Err("symbol not found".to_string())
        } else {
            Ok(s)
        }
    }
}

impl Drop for RawLib {
    /// Unmaps the library. Safe only because every `Language` derived from it
    /// is paired with an `Arc<RawLib>`, so this runs after the last one drops.
    fn drop(&mut self) {
        unsafe { sys::close(self.0) };
    }
}

#[cfg(unix)]
mod sys {
    extern "C" {
        #[cfg_attr(not(any(test, feature = "treesitter")), allow(dead_code))]
        fn dlopen(path: *const i8, flags: i32) -> *mut std::ffi::c_void;
        #[cfg_attr(not(feature = "treesitter"), allow(dead_code))]
        fn dlsym(h: *mut std::ffi::c_void, sym: *const i8) -> *mut std::ffi::c_void;
        fn dlclose(h: *mut std::ffi::c_void) -> i32;
    }
    #[cfg_attr(not(any(test, feature = "treesitter")), allow(dead_code))]
    pub unsafe fn open(path: *const i8) -> *mut std::ffi::c_void {
        dlopen(path, 2)
    } // RTLD_NOW
    #[cfg_attr(not(feature = "treesitter"), allow(dead_code))]
    pub unsafe fn sym(h: *mut std::ffi::c_void, s: *const i8) -> *mut std::ffi::c_void {
        dlsym(h, s)
    }
    pub unsafe fn close(h: *mut std::ffi::c_void) {
        dlclose(h);
    }
}

#[cfg(windows)]
mod sys {
    extern "system" {
        #[cfg_attr(not(any(test, feature = "treesitter")), allow(dead_code))]
        fn LoadLibraryA(path: *const i8) -> *mut std::ffi::c_void;
        #[cfg_attr(not(feature = "treesitter"), allow(dead_code))]
        fn GetProcAddress(h: *mut std::ffi::c_void, sym: *const i8) -> *mut std::ffi::c_void;
        fn FreeLibrary(h: *mut std::ffi::c_void) -> i32;
    }
    #[cfg_attr(not(any(test, feature = "treesitter")), allow(dead_code))]
    pub unsafe fn open(path: *const i8) -> *mut std::ffi::c_void {
        LoadLibraryA(path)
    }
    #[cfg_attr(not(feature = "treesitter"), allow(dead_code))]
    pub unsafe fn sym(h: *mut std::ffi::c_void, s: *const i8) -> *mut std::ffi::c_void {
        GetProcAddress(h, s)
    }
    pub unsafe fn close(h: *mut std::ffi::c_void) {
        FreeLibrary(h);
    }
}

#[cfg(not(any(unix, windows)))]
mod sys {
    pub unsafe fn open(_: *const i8) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
    pub unsafe fn sym(_: *mut std::ffi::c_void, _: *const i8) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
    pub unsafe fn close(_: *mut std::ffi::c_void) {}
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::RawLib;
    use std::sync::Arc;

    #[cfg(windows)]
    const ALWAYS_LOADED_LIB: &str = "kernel32.dll";
    #[cfg(all(unix, not(target_os = "macos")))]
    const ALWAYS_LOADED_LIB: &str = "libc.so.6";
    #[cfg(target_os = "macos")]
    const ALWAYS_LOADED_LIB: &str = "libSystem.B.dylib";

    /// A `Language`-bearing `Arc<RawLib>` clone must outlive the original
    /// storage slot, mirroring `register_grammar`/`load_language`.
    #[test]
    fn arc_rawlib_outlives_original_storage_slot() {
        let lib = unsafe { RawLib::open(ALWAYS_LOADED_LIB) }.expect("open a system library");
        let lib = Arc::new(lib);

        // Simulates `loaded_libs` holding one reference...
        let mut loaded_libs: Vec<Arc<RawLib>> = vec![lib.clone()];
        // ...and a `LoadedLanguage`-like clone holding another, handed out to a caller.
        let handed_out: Arc<RawLib> = lib.clone();
        drop(lib);

        assert_eq!(Arc::strong_count(&handed_out), 2);

        // Drop the loader-side storage entirely (e.g. loader itself goes away).
        loaded_libs.clear();
        drop(loaded_libs);

        // The handed-out clone must still keep the library mapped.
        assert_eq!(
            Arc::strong_count(&handed_out),
            1,
            "the library must still be alive via the outstanding Arc clone"
        );
    }
}
