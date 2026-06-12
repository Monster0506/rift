//! Typed read helpers over the generic Value payload (design.md sec 6).
//! Pure functions, never a second storage path.

use super::value::Value;

/// A generic tooltip string, if present (`payload.tooltip`).
pub fn tooltip(payload: &Value) -> Option<&str> {
    payload.get("tooltip").and_then(Value::as_str)
}

/// Helpers for `fs.*` annotation payloads (the file explorer).
pub mod fs {
    use super::Value;

    /// The stable directory-entry id (`payload.entry_id`).
    pub fn entry_id(payload: &Value) -> Option<u16> {
        payload
            .get("entry_id")
            .and_then(Value::as_int)
            .map(|i| i as u16)
    }

    /// The filesystem path for an `fs.entry` (`payload.path`).
    pub fn path(payload: &Value) -> Option<&str> {
        payload.get("path").and_then(Value::as_str)
    }

    /// The display name for an `fs.entry` (`payload.name`).
    pub fn name(payload: &Value) -> Option<&str> {
        payload.get("name").and_then(Value::as_str)
    }

    /// Whether the entry is a directory (`payload.is_dir`).
    pub fn is_dir(payload: &Value) -> Option<bool> {
        payload.get("is_dir").and_then(Value::as_bool)
    }
}

/// Helpers for `lsp.*` annotation payloads.
pub mod lsp {
    use super::Value;

    /// Diagnostic severity (`payload.severity`); LSP convention 1=error..4=hint.
    pub fn severity(payload: &Value) -> Option<i64> {
        payload.get("severity").and_then(Value::as_int)
    }

    /// Diagnostic message (`payload.message`).
    pub fn message(payload: &Value) -> Option<&str> {
        payload.get("message").and_then(Value::as_str)
    }

    /// The rendered tooltip text (`payload.tooltip`), as built by the LSP layer.
    pub fn tooltip(payload: &Value) -> Option<&str> {
        super::tooltip(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_reads_string_field() {
        let mut p = Value::map();
        p.set("tooltip", Value::Str("boom".into()));
        assert_eq!(tooltip(&p), Some("boom"));
        assert_eq!(tooltip(&Value::Null), None);
    }

    #[test]
    fn fs_entry_id_reads_int_field() {
        let mut p = Value::map();
        p.set("entry_id", Value::Int(42));
        assert_eq!(fs::entry_id(&p), Some(42));
        assert_eq!(fs::entry_id(&Value::map()), None);
    }

    #[test]
    fn lsp_severity_and_message() {
        let mut p = Value::map();
        p.set("severity", Value::Int(1));
        p.set("message", Value::Str("type mismatch".into()));
        assert_eq!(lsp::severity(&p), Some(1));
        assert_eq!(lsp::message(&p), Some("type mismatch"));
    }
}
