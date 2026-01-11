use crate::error::{ErrorType, RiftError};
use std::path::{Path, PathBuf};
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

pub struct LanguageLoader {
    _grammar_dir: PathBuf,
}

impl LanguageLoader {
    pub fn new(grammar_dir: PathBuf) -> Self {
        Self {
            _grammar_dir: grammar_dir,
        }
    }

    /// Load a language based on file extension
    pub fn load_language_for_file(&self, path: &Path) -> Result<LoadedLanguage, RiftError> {
        let extension = path.extension().and_then(|e| e.to_str()).ok_or_else(|| {
            RiftError::new(ErrorType::Internal, "NO_EXTENSION", "File has no extension")
        })?;

        let lang_name = match extension {
            "c" | "h" => "c",
            "cc" | "cpp" | "cxx" | "hpp" => "cpp",
            "css" => "css",
            "go" => "go",
            "html" => "html",
            "js" | "jsx" | "mjs" => "javascript",
            "json" => "json",
            "lua" => "lua",
            "md" | "markdown" => "markdown",
            "py" => "python",
            "rs" => "rust",
            "sh" | "bash" | "zsh" => "bash",
            "ts" => "typescript",
            "tsx" => "tsx",
            "yaml" | "yml" => "yaml",
            _ => {
                return Err(RiftError::new(
                    ErrorType::Internal,
                    "UNKNOWN_EXTENSION",
                    format!("Unknown extension: {}", extension),
                ))
            }
        };

        self.load_language(lang_name)
    }

    /// Load a specific language by name (e.g., "rust")
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

    /// Load a query file for a language (e.g., "highlights")
    #[allow(unused_variables)]
    pub fn load_query(&self, lang_name: &str, query_name: &str) -> Result<String, RiftError> {
        // Check for bundled queries first (when feature is enabled)
        if query_name == "highlights" {
            #[cfg(feature = "treesitter")]
            if let Some((_, query)) = get_bundled_language(lang_name) {
                return Ok(query.to_string());
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
}

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
        _ => None,
    }
}
