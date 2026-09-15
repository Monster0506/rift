//! Namespaced annotation kinds.
//! Open strings like "lsp.diagnostic"; bulk ops query by prefix, no closed enum.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// An annotation kind: a namespaced string like `"lsp.diagnostic"` or `"ui.button"`.
/// Backed by `Arc<str>`, so cloning it is a refcount bump, not an allocation.
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

    /// A cheap `Arc` clone of the underlying string data.
    pub fn as_arc(&self) -> Arc<str> {
        self.0.clone()
    }

    /// The leading namespace segment (text before the first `.`), e.g. `"lsp"`
    /// for `"lsp.diagnostic"`. Returns the whole string if there is no `.`.
    pub fn namespace(&self) -> &str {
        self.0.split('.').next().unwrap_or(&self.0)
    }

    /// Whether this kind equals or begins with `prefix` (e.g. "lsp." matches
    /// "lsp.diagnostic"). Basis for clear_by_kind_prefix / query_kind.
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

/// Well-known kind strings used by core subsystems. These are conveniences only:
/// nothing about storage treats them specially.
pub mod well_known {
    pub const FS_ENTRY: &str = "fs.entry";
    pub const LSP_DIAGNOSTIC: &str = "lsp.diagnostic";
    pub const LSP_HINT: &str = "lsp.hint";
    pub const GIT_BLAME: &str = "git.blame";
    /// One line of a `BufferKind::GitStatus` buffer naming a tracked/untracked
    /// path and the section it belonged to at populate/expand time.
    pub const GIT_STATUS_ENTRY: &str = "git.status_entry";
    /// The `HEAD <sha> <subject>` summary line at the top of a `BufferKind::GitStatus` buffer. Interactive: Enter opens the Log browser.
    pub const GIT_STATUS_HEAD: &str = "git.status_head";
    /// A hunk header line (`@@ ... @@`) inside an expanded status-buffer entry,
    /// anchoring the whole hunk block for the `s`/`u`/`X` cursor actions.
    pub const GIT_HUNK: &str = "git.hunk";
    /// One `+`/`-` content line inside an expanded status-buffer hunk, anchoring per-line stage/unstage/discard for the `s`/`u`/`X` cursor actions (context lines get no annotation; nothing to select).
    pub const GIT_HUNK_LINE: &str = "git.hunk_line";
    /// One commit line of a `BufferKind::GitLog` browser buffer.
    pub const GIT_LOG_COMMIT: &str = "git.log_commit";
    /// The head line of one commit in a `BufferKind::GitRebaseTodo` plan, tagging it with that commit's sha. Interactive, so `j`/`k` (via interface-mode snapping) can only land on head lines, never on a commit's inline body-preview lines.
    pub const GIT_REBASE_STEP: &str = "git.rebase_step";
    /// A per-line gutter-diff sign (add/change/delete), for tinting the
    /// line-number gutter of a `File` buffer against its git index.
    pub const GIT_GUTTER: &str = "git.gutter";
    pub const MARK_USER: &str = "mark.user";
    pub const UI_LINK: &str = "ui.link";
    pub const UI_BUTTON: &str = "ui.button";
    pub const UI_CHECKBOX: &str = "ui.checkbox";
    pub const BUFFER_ENTRY: &str = "buffer.entry";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_extracts_leading_segment() {
        assert_eq!(Kind::new("lsp.diagnostic").namespace(), "lsp");
        assert_eq!(Kind::new("ui.checkbox").namespace(), "ui");
        assert_eq!(Kind::new("bare").namespace(), "bare");
    }

    #[test]
    fn prefix_matching() {
        let k = Kind::new("lsp.diagnostic");
        assert!(k.matches_prefix("lsp."));
        assert!(k.matches_prefix("lsp.diagnostic"));
        assert!(k.matches_prefix("lsp"));
        assert!(!k.matches_prefix("git."));
        assert!(!k.matches_prefix("lsp.hint"));
    }

    #[test]
    fn serde_is_transparent_string() {
        let k = Kind::new("ui.button");
        assert_eq!(serde_json::to_string(&k).unwrap(), r#""ui.button""#);
        let back: Kind = serde_json::from_str(r#""ui.button""#).unwrap();
        assert_eq!(back, k);
    }
}
