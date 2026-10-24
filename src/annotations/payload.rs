//! Typed read helpers over the generic Value payload.
//! Pure functions, never a second storage path.

use super::value::Value;

/// A generic tooltip string, if present (`payload.tooltip`).
pub(crate) fn tooltip(payload: &Value) -> Option<&str> {
    payload.get("tooltip").and_then(Value::as_str)
}

/// Helpers for `fs.*` annotation payloads (the file explorer).
pub(crate) mod fs {
    use crate::annotations::value::Value;

    /// The stable directory-entry id (`payload.entry_id`).
    pub(in crate::annotations) fn entry_id(payload: &Value) -> Option<u16> {
        payload
            .get("entry_id")
            .and_then(Value::as_int)
            .and_then(|i| u16::try_from(i).ok())
    }

    /// The display name for an `fs.entry` (`payload.name`).
    pub(crate) fn name(payload: &Value) -> Option<&str> {
        payload.get("name").and_then(Value::as_str)
    }

    /// Whether the entry is a directory (`payload.is_dir`).
    pub(crate) fn is_dir(payload: &Value) -> Option<bool> {
        payload.get("is_dir").and_then(Value::as_bool)
    }
}

/// Helpers for `lsp.*` annotation payloads.
pub(crate) mod lsp {
    use crate::annotations::value::Value;

    /// Diagnostic severity (`payload.severity`); LSP convention 1=error..4=hint.
    pub(in crate::annotations) fn severity(payload: &Value) -> Option<i64> {
        payload.get("severity").and_then(Value::as_int)
    }

    #[cfg(test)]
    /// Diagnostic message (`payload.message`).
    pub(crate) fn message(payload: &Value) -> Option<&str> {
        payload.get("message").and_then(Value::as_str)
    }
}

/// Helpers for `git.*` annotation payloads (status buffer + hunk blocks).
pub(in crate::annotations) mod git {
    use crate::annotations::value::Value;

    /// The tracked/untracked path a `git.status_entry`/`git.hunk` line refers to
    /// (`payload.path`).
    pub(in crate::annotations) fn path(payload: &Value) -> Option<&str> {
        payload.get("path").and_then(Value::as_str)
    }

    /// The section a `git.status_entry` belonged to at populate/expand time: one of `"staged"`, `"unstaged"`, `"untracked"` (`payload.section`). Unmerged entries are never annotated for reconciliation purposes.
    pub(in crate::annotations) fn section(payload: &Value) -> Option<&str> {
        payload.get("section").and_then(Value::as_str)
    }

    /// The pre-rename path, for a `git.status_entry` on a renamed file
    /// (`payload.orig_path`).
    pub(in crate::annotations) fn orig_path(payload: &Value) -> Option<&str> {
        payload.get("orig_path").and_then(Value::as_str)
    }

    /// Whether a `git.hunk` came from the staged diff (`git diff --cached`,
    /// `true`) or the unstaged diff (`git diff`, `false`) (`payload.staged_side`).
    pub(in crate::annotations) fn staged_side(payload: &Value) -> Option<bool> {
        payload.get("staged_side").and_then(Value::as_bool)
    }

    /// Index into the expanded-diff snapshot's `Vec<Hunk>` for this path/side
    /// (`payload.hunk_index`).
    pub(in crate::annotations) fn hunk_index(payload: &Value) -> Option<usize> {
        payload
            .get("hunk_index")
            .and_then(Value::as_int)
            .and_then(|i| usize::try_from(i).ok())
    }

    /// Index into that hunk's `Vec<DiffLine>` for a `git.hunk_line`
    /// (`payload.line_index`).
    pub(in crate::annotations) fn line_index(payload: &Value) -> Option<usize> {
        payload
            .get("line_index")
            .and_then(Value::as_int)
            .and_then(|i| usize::try_from(i).ok())
    }

    /// The commit sha a `git.blame`/`git.log_commit` line refers to (`payload.sha`).
    pub(in crate::annotations) fn sha(payload: &Value) -> Option<&str> {
        payload.get("sha").and_then(Value::as_str)
    }

    /// The gutter-diff sign kind for a `git.gutter` line: `"add"`,
    /// `"change"`, or `"delete"` (`payload.gutter_kind`).
    pub(in crate::annotations) fn gutter_kind(payload: &Value) -> Option<&str> {
        payload.get("gutter_kind").and_then(Value::as_str)
    }
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod payload_tests;
