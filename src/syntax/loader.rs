use crate::error::{ErrorType, RiftError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tree_sitter::Language;

/// Handle to a loaded language, keeping the library alive
pub struct LoadedLanguage {
    pub language: Language,
    pub name: String,
}

impl LoadedLanguage {
    /// Create a LoadedLanguage for bundled grammars (no library handle needed)
    #[allow(dead_code)]
    pub fn bundled(language: Language, name: &str) -> Self {
        Self {
            language,
            name: name.to_string(),
        }
    }
}

/// Runtime-registered language customisations from plugins.
#[derive(Default)]
pub struct DynamicRegistry {
    /// File extension → language name overrides (e.g. "svelte" → "svelte").
    pub filetype_map: HashMap<String, String>,
    /// Language name → highlights query source override.
    pub highlights_overrides: HashMap<String, String>,
    /// Language name → injections query source.
    pub injections_queries: HashMap<String, String>,
}

pub struct LanguageLoader {
    _grammar_dir: PathBuf,
    pub dynamic: RwLock<DynamicRegistry>,
}

impl LanguageLoader {
    pub fn new(grammar_dir: PathBuf) -> Self {
        Self {
            _grammar_dir: grammar_dir,
            dynamic: RwLock::new(DynamicRegistry::default()),
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
/// Our simplified injection protocol maps capture name → language,
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
