use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Kind(Arc<str>);

impl Kind {
    pub fn new(s: impl Into<String>) -> Self {
        Self(Arc::from(s.into()))
    }

    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub(crate) fn as_arc(&self) -> Arc<str> {
        self.0.clone()
    }

    #[must_use]
    pub(crate) fn namespace(&self) -> &str {
        self.0.split('.').next().unwrap_or(&self.0)
    }

    #[must_use]
    pub(crate) fn matches_prefix(&self, prefix: &str) -> bool {
        &*self.0 == prefix || self.0.starts_with(prefix)
    }
}

impl From<&str> for Kind {
    fn from(s: &str) -> Self {
        Self(Arc::from(s))
    }
}

impl From<String> for Kind {
    fn from(s: String) -> Self {
        Self(Arc::from(s))
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub(crate) mod well_known {
    pub(crate) const FS_ENTRY: &str = "fs.entry";
    pub(crate) const LSP_DIAGNOSTIC: &str = "lsp.diagnostic";
    pub(crate) const GIT_BLAME: &str = "git.blame";
    pub(crate) const GIT_STATUS_ENTRY: &str = "git.status_entry";
    pub(crate) const GIT_STATUS_HEAD: &str = "git.status_head";
    pub(crate) const GIT_HUNK: &str = "git.hunk";
    pub(crate) const GIT_HUNK_LINE: &str = "git.hunk_line";
    pub(crate) const GIT_LOG_COMMIT: &str = "git.log_commit";
    pub(crate) const GIT_REBASE_STEP: &str = "git.rebase_step";
    pub(crate) const GIT_GUTTER: &str = "git.gutter";
    pub(crate) const BUFFER_ENTRY: &str = "buffer.entry";
    pub(crate) const PLUGIN_HIGHLIGHT: &str = "plugin.highlight";
}

#[cfg(test)]
#[path = "kind_tests.rs"]
mod kind_tests;
