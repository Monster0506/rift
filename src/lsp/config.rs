/// A capability declared by a language server plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LspCapability {
    GotoDefinition,
    References,
    Hover,
    Rename,
    Format,
    Diagnostics,
    CodeActions,
}

impl LspCapability {
    #[must_use]
    pub(crate) fn parse(s: &str) -> Option<Self> {
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
pub(crate) struct LspServerConfig {
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    /// Project root markers, e.g. `["Cargo.toml", ".git"]`
    pub(crate) root_markers: Vec<String>,
    /// Capabilities declared by this server.
    pub(crate) capabilities: Vec<LspCapability>,
    /// Passed verbatim as `initializationOptions` in the LSP initialize request.
    pub(crate) initialization_options: Option<serde_json::Value>,
    /// Keep the server alive in a broker across editor sessions, so restarts
    /// reattach to its warm index instead of paying a full reindex.
    pub(crate) keep_alive: bool,
}

impl LspServerConfig {
    #[must_use]
    pub(crate) fn has_capability(&self, cap: LspCapability) -> bool {
        self.capabilities.is_empty() || self.capabilities.contains(&cap)
    }
}

/// Derive the LSP languageId from a filetype/language name.
#[must_use]
pub(crate) fn language_id(lang: &str) -> &'static str {
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
