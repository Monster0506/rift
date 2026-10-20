use crate::error::{ErrorType, RiftError};
use std::collections::HashMap;
#[cfg(any(feature = "treesitter", all(test, any(unix, windows))))]
use std::ffi::CString;
use std::path::{Path, PathBuf};
#[cfg(feature = "treesitter")]
use std::sync::Mutex;
use std::sync::{Arc, RwLock};

#[cfg(feature = "treesitter")]
pub type Language = tree_sitter::Language;
#[cfg(not(feature = "treesitter"))]
#[derive(Clone)]
pub struct Language;

#[derive(Clone)]
pub struct LoadedLanguage {
    pub language: Language,
    pub name: String,
    pub lib: Option<Arc<RawLib>>,
}

impl LoadedLanguage {
    #[cfg(feature = "treesitter")]
    #[must_use]
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
    #[cfg(feature = "treesitter")]
    loaded_libs: Mutex<Vec<Arc<RawLib>>>,
    /// Grammars registered via `register_grammar()`, keyed by language name.
    dynamic_languages: RwLock<HashMap<String, LoadedLanguage>>,
}

impl LanguageLoader {
    #[must_use]
    pub fn new(grammar_dir: PathBuf) -> Self {
        Self {
            _grammar_dir: grammar_dir,
            dynamic: RwLock::new(DynamicRegistry::default()),
            #[cfg(feature = "treesitter")]
            loaded_libs: Mutex::new(Vec::new()),
            dynamic_languages: RwLock::new(HashMap::new()),
        }
    }

    // Dynamic registration (called from plugin mutations)

    pub fn register_filetype(&self, ext: &str, lang_name: &str) {
        let mut reg = self
            .dynamic
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reg.filetype_map
            .insert(ext.to_string(), lang_name.to_string());
    }

    pub fn language_name_for_file(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        // Dynamic registry (from plugins) takes priority.
        {
            let reg = self
                .dynamic
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(name) = reg.filetype_map.get(ext) {
                return Some(name.clone());
            }
        }
        // Built-in extension map.
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
        let mut reg = self
            .dynamic
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reg.highlights_overrides
            .insert(lang_name.to_string(), query_src.to_string());
    }

    pub fn register_injections_query(&self, lang_name: &str, query_src: &str) {
        let mut reg = self
            .dynamic
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reg.injections_queries
            .insert(lang_name.to_string(), query_src.to_string());
    }

    /// Load a grammar shared object and register it under `lang_name`.
    ///
    /// # Errors
    /// Returns `Err` when the library or its language symbol cannot be loaded.
    #[cfg(feature = "treesitter")]
    pub fn register_grammar(
        &self,
        lang_name: &str,
        so_path: &str,
        fn_name: &str,
    ) -> Result<(), String> {
        type LangFn = unsafe extern "C" fn() -> *const tree_sitter::ffi::TSLanguage;

        if self
            .dynamic_languages
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(lang_name)
        {
            return Ok(());
        }

        // SAFETY: so_path is a user-installed grammar library; dlopen runs its initializers.
        let lib =
            unsafe { RawLib::open(so_path) }.map_err(|e| format!("dlopen '{so_path}': {e}"))?;

        // SAFETY: lib holds a live handle from open; CString rejects interior NULs.
        let sym = unsafe { lib.sym(fn_name.as_bytes()) }
            .map_err(|e| format!("symbol '{fn_name}' in '{so_path}': {e}"))?;
        // SAFETY: tree-sitter grammars export this symbol with the LangFn signature.
        let f: LangFn = unsafe { std::mem::transmute(sym) };
        // SAFETY: f is the grammar entry point and lib stays loaded for this call.
        let ptr = unsafe { f() };
        if ptr.is_null() {
            return Err(format!("'{fn_name}' returned a null TSLanguage pointer"));
        }
        // SAFETY: ptr is non-null static grammar data kept alive by lib.
        let language: Language = unsafe { Language::from_raw(ptr) };

        let lib = Arc::new(lib);

        self.loaded_libs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(lib.clone());

        self.dynamic_languages
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
    ///
    /// # Errors
    /// Always returns `Err`.
    #[cfg(not(feature = "treesitter"))]
    pub fn register_grammar(
        &self,
        _lang_name: &str,
        _so_path: &str,
        _fn_name: &str,
    ) -> Result<(), String> {
        Err("tree-sitter support is not compiled in".to_string())
    }

    // Language loading

    /// Load a language based on file extension, checking the dynamic registry first.
    ///
    /// # Errors
    /// Returns `Err` when the path has no extension or its language cannot be loaded.
    pub fn load_language_for_file(&self, path: &Path) -> Result<LoadedLanguage, RiftError> {
        let extension = path.extension().and_then(|e| e.to_str()).ok_or_else(|| {
            RiftError::new(ErrorType::Internal, "NO_EXTENSION", "File has no extension")
        })?;

        // Dynamic registry takes priority.
        let dyn_lang = {
            let reg = self
                .dynamic
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                        format!("Unknown extension: {extension}"),
                    ))
                }
            }
        };

        self.load_language(lang_name)
    }

    /// Load a specific language by name (e.g., "rust").
    ///
    /// # Errors
    /// Returns `Err` when neither a dynamic nor a bundled grammar exists for `lang_name`.
    pub fn load_language(&self, lang_name: &str) -> Result<LoadedLanguage, RiftError> {
        {
            let langs = self
                .dynamic_languages
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
            format!("Language {lang_name} not found or feature not enabled"),
        ))
    }

    // Query loading

    /// Load a highlights query, checking dynamic overrides first.
    ///
    /// # Errors
    /// Returns `Err` when no highlights override or bundled query exists for the language.
    pub fn load_query(&self, lang_name: &str, query_name: &str) -> Result<String, RiftError> {
        // Check highlights overrides from plugins.
        if query_name == "highlights" {
            let reg = self
                .dynamic
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
            if lang_name == "rust" {
                return Ok(format!(
                    "{}\n{}",
                    tree_sitter_rust::HIGHLIGHTS_QUERY,
                    concat!(
                        "(let_declaration pattern: (identifier) @variable)\n",
                        "(for_expression pattern: (identifier) @variable)\n",
                        "(let_condition pattern: (identifier) @variable)\n",
                        "(closure_parameters (identifier) @variable)\n",
                        "(match_pattern (identifier) @variable)\n",
                        "(tuple_pattern (identifier) @variable)"
                    )
                ));
            }

            #[cfg(feature = "treesitter")]
            if lang_name == "bash" {
                return Ok(format!(
                    "{}\n{}",
                    tree_sitter_bash::HIGHLIGHT_QUERY,
                    "(variable_name) @variable"
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
            format!("Query {query_name} for {lang_name} not found (bundled only)"),
        ))
    }

    /// Load an injections query for a language.
    /// Checks the dynamic plugin registry first, then bundled queries.
    pub fn load_injections_query(&self, lang_name: &str) -> Option<String> {
        // Plugin-registered injections take priority.
        {
            let reg = self
                .dynamic
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(q) = reg.injections_queries.get(lang_name) {
                return Some(q.clone());
            }
        }

        // Bundled injections.
        get_bundled_injections_query(lang_name).map(str::to_string)
    }
}

// Bundled grammar tables

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
        "c_sharp" => Some((
            tree_sitter_c_sharp::LANGUAGE.into(),
            tree_sitter_c_sharp::HIGHLIGHTS_QUERY,
        )),
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

/// Built-in injections queries for bundled grammars. The capture name is the
/// target language, avoiding the need to parse `#set! injection.language`.
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

pub struct RawLib(*mut std::ffi::c_void);

// SAFETY: dlopen/LoadLibrary handles are process-wide; dlsym/dlclose are thread-safe.
unsafe impl Send for RawLib {}
// SAFETY: RawLib is never mutated after open, so shared &self access is sound.
unsafe impl Sync for RawLib {}

impl RawLib {
    #[cfg(any(feature = "treesitter", all(test, any(unix, windows))))]
    unsafe fn open(path: &str) -> Result<Self, String> {
        let c = CString::new(path).map_err(|e| e.to_string())?;
        let h = sys::open(c.as_ptr());
        if h.is_null() {
            Err(format!("could not open '{path}'"))
        } else {
            Ok(Self(h))
        }
    }

    #[cfg(feature = "treesitter")]
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
    fn drop(&mut self) {
        // SAFETY: self.0 came from a successful open, and Drop runs exactly once.
        unsafe { sys::close(self.0) };
    }
}

#[cfg(unix)]
mod sys {
    extern "C" {
        #[cfg(any(feature = "treesitter", all(test, any(unix, windows))))]
        fn dlopen(path: *const i8, flags: i32) -> *mut std::ffi::c_void;
        #[cfg(feature = "treesitter")]
        fn dlsym(h: *mut std::ffi::c_void, sym: *const i8) -> *mut std::ffi::c_void;
        fn dlclose(h: *mut std::ffi::c_void) -> i32;
    }
    #[cfg(any(feature = "treesitter", all(test, any(unix, windows))))]
    pub unsafe fn open(path: *const i8) -> *mut std::ffi::c_void {
        dlopen(path, 2)
    } // RTLD_NOW
    #[cfg(feature = "treesitter")]
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
        #[cfg(any(feature = "treesitter", all(test, any(unix, windows))))]
        fn LoadLibraryA(path: *const i8) -> *mut std::ffi::c_void;
        #[cfg(feature = "treesitter")]
        fn GetProcAddress(h: *mut std::ffi::c_void, sym: *const i8) -> *mut std::ffi::c_void;
        fn FreeLibrary(h: *mut std::ffi::c_void) -> i32;
    }
    #[cfg(any(feature = "treesitter", all(test, any(unix, windows))))]
    pub unsafe fn open(path: *const i8) -> *mut std::ffi::c_void {
        LoadLibraryA(path)
    }
    #[cfg(feature = "treesitter")]
    pub unsafe fn sym(h: *mut std::ffi::c_void, s: *const i8) -> *mut std::ffi::c_void {
        GetProcAddress(h, s)
    }
    pub unsafe fn close(h: *mut std::ffi::c_void) {
        FreeLibrary(h);
    }
}

#[cfg(not(any(unix, windows)))]
mod sys {
    #[cfg(any(feature = "treesitter", all(test, any(unix, windows))))]
    pub unsafe fn open(_: *const i8) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
    #[cfg(feature = "treesitter")]
    pub unsafe fn sym(_: *mut std::ffi::c_void, _: *const i8) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
    pub unsafe fn close(_: *mut std::ffi::c_void) {}
}

#[cfg(test)]
#[cfg(any(unix, windows))]
#[path = "loader_tests.rs"]
mod loader_tests;
