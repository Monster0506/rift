use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Kind(Arc<str>);

impl Kind {
    pub fn new(s: impl Into<String>) -> Self {
        Kind(Arc::from(s.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_arc(&self) -> Arc<str> {
        self.0.clone()
    }

    pub fn namespace(&self) -> &str {
        self.0.split('.').next().unwrap_or(&self.0)
    }

    pub fn matches_prefix(&self, prefix: &str) -> bool {
        &*self.0 == prefix || self.0.starts_with(prefix)
    }
}

impl From<&str> for Kind {
    fn from(s: &str) -> Self {
        Kind(Arc::from(s))
    }
}

impl From<String> for Kind {
    fn from(s: String) -> Self {
        Kind(Arc::from(s))
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub mod well_known {
    pub const FS_ENTRY: &str = "fs.entry";
    pub const LSP_DIAGNOSTIC: &str = "lsp.diagnostic";
    pub const LSP_HINT: &str = "lsp.hint";
    pub const GIT_BLAME: &str = "git.blame";
    pub const GIT_STATUS_ENTRY: &str = "git.status_entry";
    pub const GIT_STATUS_HEAD: &str = "git.status_head";
    pub const GIT_HUNK: &str = "git.hunk";
    pub const GIT_HUNK_LINE: &str = "git.hunk_line";
    pub const GIT_LOG_COMMIT: &str = "git.log_commit";
    pub const GIT_REBASE_STEP: &str = "git.rebase_step";
    pub const GIT_GUTTER: &str = "git.gutter";
    pub const MARK_USER: &str = "mark.user";
    pub const UI_LINK: &str = "ui.link";
    pub const UI_BUTTON: &str = "ui.button";
    pub const UI_CHECKBOX: &str = "ui.checkbox";
    pub const BUFFER_ENTRY: &str = "buffer.entry";
    pub const PLUGIN_HIGHLIGHT: &str = "plugin.highlight";
}

#[cfg(test)]
#[path = "kind_tests.rs"]
mod kind_tests;
