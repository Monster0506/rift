use crate::error::{ErrorType, RiftError};
use libloading::{Library, Symbol};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tree_sitter::Language;

/// Handle to a loaded language, keeping the library alive
pub struct LoadedLanguage {
    pub language: Language,
    /// Library handle - None for bundled grammars, Some for dynamically loaded
    pub library: Option<Arc<Library>>,
    pub name: String,
}

impl LoadedLanguage {
    /// Create a LoadedLanguage for bundled grammars (no library handle needed)
    #[allow(dead_code)]
    pub fn bundled(language: Language, name: &str) -> Self {
        Self {
            language,
            library: None,
            name: name.to_string(),
        }
    }
}

pub struct LanguageLoader {
    grammar_dir: PathBuf,
}

impl LanguageLoader {
    pub fn new(grammar_dir: PathBuf) -> Self {
        Self { grammar_dir }
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
    pub fn load_language(&self, lang_name: &str) -> Result<LoadedLanguage, RiftError> {
        // Check for bundled grammars first (when feature is enabled)
        #[cfg(feature = "bundled-rust")]
        if lang_name == "rust" {
            return Ok(LoadedLanguage::bundled(
                tree_sitter_rust::LANGUAGE.into(),
                "rust",
            ));
        }

        #[cfg(feature = "bundled-python")]
        if lang_name == "python" {
            return Ok(LoadedLanguage::bundled(
                tree_sitter_python::LANGUAGE.into(),
                "python",
            ));
        }

        // Fall back to dynamic loading
        self.load_language_dynamic(lang_name)
    }

    /// Load a language dynamically from a shared library
    fn load_language_dynamic(&self, lang_name: &str) -> Result<LoadedLanguage, RiftError> {
        // Platform specific filename
        #[cfg(target_os = "windows")]
        let filename = format!("tree-sitter-{}.dll", lang_name);
        #[cfg(not(target_os = "windows"))]
        let filename = format!("libtree-sitter-{}.so", lang_name);

        let library_path = self.grammar_dir.join(&filename);

        println!("Loading grammar from: {:?}", library_path); // DEBUG

        if !library_path.exists() {
            return Err(RiftError::new(
                ErrorType::Io,
                "GRAMMAR_NOT_FOUND",
                format!("Grammar file not found: {:?}", library_path),
            ));
        }

        unsafe {
            let library = Library::new(&library_path).map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    "LOAD_FAILED",
                    format!("Failed to load library: {}", e),
                )
            })?;

            let library = Arc::new(library);

            // Symbol name is usually tree_sitter_<lang_name>
            let symbol_name = format!("tree_sitter_{}", lang_name.replace("-", "_"));
            let constructor: Symbol<unsafe extern "C" fn() -> Language> =
                library.get(symbol_name.as_bytes()).map_err(|e| {
                    RiftError::new(
                        ErrorType::Internal,
                        "SYMBOL_NOT_FOUND",
                        format!("Symbol {} not found: {}", symbol_name, e),
                    )
                })?;

            let language = constructor();

            Ok(LoadedLanguage {
                language,
                library: Some(library),
                name: lang_name.to_string(),
            })
        }
    }

    /// Load a query file for a language (e.g., "highlights")
    pub fn load_query(&self, lang_name: &str, query_name: &str) -> Result<String, RiftError> {
        // Check for bundled queries first (when feature is enabled)
        if query_name == "highlights" {
            #[cfg(feature = "bundled-rust")]
            if lang_name == "rust" {
                return Ok(tree_sitter_rust::HIGHLIGHTS_QUERY.to_string());
            }

            #[cfg(feature = "bundled-python")]
            if lang_name == "python" {
                return Ok(tree_sitter_python::HIGHLIGHTS_QUERY.to_string());
            }
        }

        let filename = format!("{}.scm", query_name);
        // Search structure: grammar_dir/queries/lang/query.scm OR grammar_dir/lang/query.scm
        let paths = [
            self.grammar_dir
                .join("queries")
                .join(lang_name)
                .join(&filename),
            self.grammar_dir.join(lang_name).join(&filename),
        ];

        for path in &paths {
            if path.exists() {
                return std::fs::read_to_string(path).map_err(|e| {
                    RiftError::new(
                        ErrorType::Io,
                        "READ_FAILED",
                        format!("Failed to read query file {:?}: {}", path, e),
                    )
                });
            }
        }

        Err(RiftError::new(
            ErrorType::Io,
            "QUERY_NOT_FOUND",
            format!(
                "Query {} for {} not found in grammar dir {:?}",
                query_name, lang_name, self.grammar_dir
            ),
        ))
    }
}
