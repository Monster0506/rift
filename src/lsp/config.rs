/// A capability declared by a language server plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspCapability {
    GotoDefinition,
    References,
    Hover,
    Rename,
    Format,
    Diagnostics,
    CodeActions,
}

impl LspCapability {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "definition" => Some(Self::GotoDefinition),
            "references" => Some(Self::References),
            "hover" => Some(Self::Hover),
            "rename" => Some(Self::Rename),
            "format" => Some(Self::Format),
            "diagnostics" => Some(Self::Diagnostics),
            "code_actions" => Some(Self::CodeActions),
            _ => None,
        }
    }
}

/// Configuration for a language server registered by a plugin.
#[derive(Debug, Clone)]
pub struct LspServerConfig {
    pub command: String,
    pub args: Vec<String>,
    /// File extensions this server handles, e.g. `[".rs"]`
    pub extensions: Vec<String>,
    /// Project root markers, e.g. `["Cargo.toml", ".git"]`
    pub root_markers: Vec<String>,
    /// Capabilities declared by this server.
    pub capabilities: Vec<LspCapability>,
}

impl LspServerConfig {
    pub fn has_capability(&self, cap: &LspCapability) -> bool {
        self.capabilities.is_empty() || self.capabilities.contains(cap)
    }
}

/// Derive the LSP languageId from a filetype/language name.
pub fn language_id(lang: &str) -> &'static str {
    match lang {
        "rust" => "rust",
        "python" => "python",
        "typescript" => "typescript",
        "javascript" => "javascript",
        "go" => "go",
        "lua" => "lua",
        "c" => "c",
        "cpp" => "cpp",
        "zig" => "zig",
        "bash" => "shellscript",
        "html" => "html",
        "css" => "css",
        "json" => "json",
        "yaml" => "yaml",
        "markdown" | "md" => "markdown",
        _ => "plaintext",
    }
}
