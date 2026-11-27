#[cfg(feature = "treesitter")]
use crate::error::{ErrorType, RiftError};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
#[cfg(feature = "treesitter")]
use std::sync::Arc;
use std::sync::RwLock;

#[cfg(feature = "treesitter")]
type Language = tree_sitter::Language;

#[cfg(feature = "treesitter")]
#[derive(Clone)]
pub(crate) struct LoadedLanguage {
    pub(crate) language: Language,
    pub(crate) name: String,
    pub(super) lib: Option<Arc<libloading::Library>>,
}

#[cfg(feature = "treesitter")]
impl LoadedLanguage {
    #[must_use]
    pub(crate) fn bundled(language: Language, name: &str) -> Self {
        Self {
            language,
            name: name.to_string(),
            lib: None,
        }
    }
}

/// Runtime-registered language customisations from plugins.
#[derive(Default)]
struct DynamicRegistry {
    /// File extension -> language name overrides (e.g. "svelte" -> "svelte").
    filetype_map: HashMap<String, String>,
    /// Language name -> highlights query source override.
    #[cfg(feature = "treesitter")]
    highlights_overrides: HashMap<String, String>,
    /// Language name -> injections query source.
    #[cfg(feature = "treesitter")]
    injections_queries: HashMap<String, String>,
}

pub(crate) struct LanguageLoader {
    _grammar_dir: PathBuf,
    dynamic: RwLock<DynamicRegistry>,
    /// Grammars registered via `register_grammar()`, keyed by language name.
    #[cfg(feature = "treesitter")]
    dynamic_languages: RwLock<HashMap<String, LoadedLanguage>>,
}

impl LanguageLoader {
    #[must_use]
    pub(crate) fn new(grammar_dir: PathBuf) -> Self {
        Self {
            _grammar_dir: grammar_dir,
            dynamic: RwLock::new(DynamicRegistry::default()),
            #[cfg(feature = "treesitter")]
            dynamic_languages: RwLock::new(HashMap::new()),
        }
    }

    // Dynamic registration (called from plugin mutations)

    #[cfg(feature = "plugins")]
    pub(crate) fn register_filetype(&self, ext: &str, lang_name: &str) {
        let mut reg = self
            .dynamic
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reg.filetype_map
            .insert(ext.to_string(), lang_name.to_string());
    }

    #[cfg(any(feature = "plugins", feature = "lsp"))]
    pub(crate) fn language_name_for_file(&self, path: &Path) -> Option<String> {
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

    #[cfg(feature = "treesitter")]
    #[cfg(feature = "plugins")]
    pub(crate) fn set_query_override(
        &self,
        lang_name: &str,
        kind: QueryKind,
        source: Option<String>,
    ) {
        let mut reg = self
            .dynamic
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let map = match kind {
            QueryKind::Highlights => &mut reg.highlights_overrides,
            QueryKind::Injections => &mut reg.injections_queries,
        };
        match source {
            Some(source) => map.insert(lang_name.to_string(), source),
            None => map.remove(lang_name),
        };
    }

    /// Load a grammar shared object and register it under `lang_name`.
    ///
    /// # Errors
    /// Returns `Err` when the library or its language symbol cannot be loaded.
    #[cfg(feature = "treesitter")]
    #[cfg(feature = "plugins")]
    pub(crate) fn register_grammar(
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

        // SAFETY: so_path is a user-installed grammar library; loading runs its initializers.
        let lib = unsafe { libloading::Library::new(so_path) }
            .map_err(|e| format!("load '{so_path}': {e}"))?;

        let ptr = {
            // SAFETY: tree-sitter grammars export this symbol with the LangFn signature.
            let entry = unsafe { lib.get::<LangFn>(fn_name.as_bytes()) }
                .map_err(|e| format!("symbol '{fn_name}' in '{so_path}': {e}"))?;
            // SAFETY: entry is the grammar entry point and lib stays loaded for this call.
            unsafe { entry() }
        };
        if ptr.is_null() {
            return Err(format!("'{fn_name}' returned a null TSLanguage pointer"));
        }
        // SAFETY: ptr is non-null static grammar data kept alive by lib.
        let language: Language = unsafe { Language::from_raw(ptr) };

        self.dynamic_languages
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                lang_name.to_string(),
                LoadedLanguage {
                    language,
                    name: lang_name.to_string(),
                    lib: Some(Arc::new(lib)),
                },
            );

        Ok(())
    }

    // Language loading

    /// Load a language based on file extension, checking the dynamic registry first.
    ///
    /// # Errors
    /// Returns `Err` when the path has no extension or its language cannot be loaded.
    #[cfg(feature = "treesitter")]
    pub(crate) fn load_language_for_file(&self, path: &Path) -> Result<LoadedLanguage, RiftError> {
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
    #[cfg(feature = "treesitter")]
    pub(crate) fn load_language(&self, lang_name: &str) -> Result<LoadedLanguage, RiftError> {
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
    #[cfg(feature = "treesitter")]
    pub(crate) fn load_query(
        &self,
        lang_name: &str,
        query_name: &str,
    ) -> Result<String, RiftError> {
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
    #[cfg(feature = "treesitter")]
    pub(crate) fn load_injections_query(&self, lang_name: &str) -> Option<String> {
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

    #[cfg(feature = "treesitter")]
    #[cfg(feature = "plugins")]
    pub(crate) fn query_source(&self, lang_name: &str, kind: QueryKind) -> Option<String> {
        match kind {
            QueryKind::Highlights => self.load_query(lang_name, "highlights").ok(),
            QueryKind::Injections => self.load_injections_query(lang_name),
        }
    }

    #[cfg(feature = "treesitter")]
    #[cfg(feature = "plugins")]
    pub(crate) fn available_languages(&self) -> Vec<String> {
        let mut names: Vec<String> = BUNDLED_LANGUAGES.iter().map(|s| (*s).to_string()).collect();
        names.extend(
            self.dynamic_languages
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .keys()
                .cloned(),
        );
        names.sort();
        names.dedup();
        names
    }
}

#[cfg(feature = "treesitter")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg(feature = "plugins")]
pub(crate) enum QueryKind {
    Highlights,
    Injections,
}

#[cfg(all(feature = "treesitter", feature = "plugins"))]
impl QueryKind {
    #[must_use]
    #[cfg(feature = "plugins")]
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "highlights" => Some(Self::Highlights),
            "injections" => Some(Self::Injections),
            _ => None,
        }
    }
}

#[cfg(feature = "treesitter")]
#[cfg(feature = "plugins")]
pub(crate) const BUNDLED_LANGUAGES: &[&str] = &[
    "bash",
    "c",
    "c_sharp",
    "cpp",
    "css",
    "go",
    "html",
    "java",
    "javascript",
    "json",
    "lua",
    "markdown",
    "php",
    "python",
    "ruby",
    "rust",
    "sql",
    "svelte",
    "tsx",
    "typescript",
    "yaml",
    "zig",
];

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
#[cfg(feature = "treesitter")]
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

#[cfg(all(test, feature = "treesitter", feature = "plugins"))]
#[path = "loader_tests.rs"]
mod loader_tests;
