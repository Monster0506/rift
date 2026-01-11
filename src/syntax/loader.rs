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
            "rs" => "rust",
            "c" => "c",
            "cc" | "cpp" | "cxx" => "cpp",
            "py" => "python",
            "js" => "javascript",
            "ts" => "typescript",
            "go" => "go",
            "html" => "html",
            "css" => "css",
            "json" => "json",
            "lua" => "lua",
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
        {
            if lang_name == "rust" {
                return Ok(LoadedLanguage::bundled(
                    tree_sitter_rust::LANGUAGE.into(),
                    "rust",
                ));
            }

            if lang_name == "python" {
                return Ok(LoadedLanguage::bundled(
                    tree_sitter_python::LANGUAGE.into(),
                    "python",
                ));
            }
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
            {
                if lang_name == "rust" {
                    return Ok(tree_sitter_rust::HIGHLIGHTS_QUERY.to_string());
                } else if lang_name == "python" {
                    return Ok(tree_sitter_python::HIGHLIGHTS_QUERY.to_string());
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
}
