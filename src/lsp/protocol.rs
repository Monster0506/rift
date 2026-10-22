use serde::{Deserialize, Serialize};
use serde_json::Value;

/// UTF-16 code units needed to encode `ch` (1, or 2 for chars outside the BMP).
const fn utf16_len(ch: char) -> u32 {
    if (ch as u32) > 0xFFFF {
        2
    } else {
        1
    }
}

/// Convert a code-point offset within a line to a UTF-16 code-unit offset,
/// the unit LSP `Position.character` is measured in over the wire.
pub(crate) fn char_offset_to_utf16(line: impl Iterator<Item = char>, char_offset: usize) -> u32 {
    line.take(char_offset).map(utf16_len).sum()
}

/// Convert a UTF-16 code-unit offset (as received from an LSP server) to a
/// code-point offset within a line.
pub(crate) fn utf16_offset_to_char(line: impl Iterator<Item = char>, utf16_offset: u32) -> usize {
    let mut units = 0u32;
    let mut chars = 0usize;
    for ch in line {
        if units >= utf16_offset {
            break;
        }
        units += utf16_len(ch);
        chars += 1;
    }
    chars
}

/// Convert a code-point offset within a line to a UTF-8 byte offset.
pub(crate) fn char_offset_to_utf8(line: impl Iterator<Item = char>, char_offset: usize) -> u32 {
    line.take(char_offset)
        .map(|c| u32::try_from(c.len_utf8()).unwrap_or(u32::MAX))
        .sum()
}

/// Convert a UTF-8 byte offset (as received from an LSP server) to a
/// code-point offset within a line.
pub(crate) fn utf8_offset_to_char(line: impl Iterator<Item = char>, byte_offset: u32) -> usize {
    let mut bytes = 0u32;
    let mut chars = 0usize;
    for ch in line {
        if bytes >= byte_offset {
            break;
        }
        bytes += u32::try_from(ch.len_utf8()).unwrap_or(u32::MAX);
        chars += 1;
    }
    chars
}

/// The unit an LSP server measures `Position.character` in, negotiated via
/// the initialize response's `positionEncoding` (defaults to UTF-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PositionEncoding {
    #[default]
    Utf16,
    Utf8,
}

impl PositionEncoding {
    #[must_use]
    pub(crate) fn from_wire(s: &str) -> Option<Self> {
        match s {
            "utf-16" => Some(Self::Utf16),
            "utf-8" => Some(Self::Utf8),
            _ => None,
        }
    }

    pub(crate) fn char_offset_in_line(self, line: impl Iterator<Item = char>, units: u32) -> usize {
        match self {
            Self::Utf16 => utf16_offset_to_char(line, units),
            Self::Utf8 => utf8_offset_to_char(line, units),
        }
    }

    pub(crate) fn units_for_char_offset(
        self,
        line: impl Iterator<Item = char>,
        char_offset: usize,
    ) -> u32 {
        match self {
            Self::Utf16 => char_offset_to_utf16(line, char_offset),
            Self::Utf8 => char_offset_to_utf8(line, char_offset),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct JsonRpcRequest {
    pub(crate) jsonrpc: &'static str,
    pub(crate) id: u64,
    pub(crate) method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct JsonRpcNotification {
    pub(crate) jsonrpc: &'static str,
    pub(crate) method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) params: Option<Value>,
}

/// A JSON-RPC request/response id, which the spec permits to be either a
/// number or a string. Server-initiated requests may use either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub(crate) enum RequestId {
    Number(u64),
    String(String),
}

#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcMessage {
    pub(crate) id: Option<Value>,
    pub(crate) method: Option<String>,
    pub(crate) params: Option<Value>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcError {
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LspPosition {
    pub(crate) line: u32,
    pub(crate) character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LspRange {
    pub(crate) start: LspPosition,
    pub(crate) end: LspPosition,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LspLocation {
    pub(crate) uri: String,
    pub(crate) range: LspRange,
}

/// `LocationLink`, the alternative shape servers may return for definition
/// requests; collapses to the target's selection range.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names)]
pub(crate) struct LspLocationLink {
    pub(crate) target_uri: String,
    pub(crate) target_range: LspRange,
    pub(crate) target_selection_range: Option<LspRange>,
}

impl From<LspLocationLink> for LspLocation {
    fn from(link: LspLocationLink) -> Self {
        Self {
            uri: link.target_uri,
            range: link.target_selection_range.unwrap_or(link.target_range),
        }
    }
}

/// Parse one `Location | LocationLink` value.
pub(crate) fn location_from_value(v: Value) -> Option<LspLocation> {
    if v.get("targetUri").is_some() {
        serde_json::from_value::<LspLocationLink>(v)
            .ok()
            .map(Into::into)
    } else {
        serde_json::from_value(v).ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LspDiagnostic {
    pub(crate) range: LspRange,
    #[serde(default)]
    pub(crate) severity: Option<u32>,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LspTextEdit {
    pub(crate) range: LspRange,
    #[serde(rename = "newText")]
    pub(crate) new_text: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceFolder {
    pub(crate) uri: String,
    pub(crate) name: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct InitializeParams {
    #[serde(rename = "processId")]
    pub(crate) process_id: Option<u32>,
    #[serde(rename = "rootUri")]
    pub(crate) root_uri: Option<String>,
    #[serde(rename = "workspaceFolders")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_folders: Option<Vec<WorkspaceFolder>>,
    pub(crate) capabilities: ClientCapabilities,
    #[serde(rename = "initializationOptions")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) initialization_options: Option<Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClientCapabilities {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentClientCapabilities,
    pub(crate) workspace: WorkspaceClientCapabilities,
    pub(crate) general: GeneralCapabilities,
    pub(crate) window: WindowCapabilities,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceClientCapabilities {
    /// We answer `workspace/applyEdit` by applying the edit in the editor.
    #[serde(rename = "applyEdit")]
    pub(crate) apply_edit: bool,
    #[serde(rename = "workspaceEdit")]
    pub(crate) workspace_edit: WorkspaceEditClientCapabilities,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceEditClientCapabilities {
    #[serde(rename = "documentChanges")]
    pub(crate) document_changes: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct WindowCapabilities {
    #[serde(rename = "workDoneProgress")]
    pub(crate) work_done_progress: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct GeneralCapabilities {
    #[serde(rename = "positionEncodings")]
    pub(crate) position_encodings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TextDocumentClientCapabilities {
    pub(crate) synchronization: TextDocumentSyncClientCapabilities,
    pub(crate) hover: HoverClientCapabilities,
    pub(crate) definition: DefinitionClientCapabilities,
    pub(crate) references: ReferencesClientCapabilities,
    pub(crate) rename: RenameClientCapabilities,
    pub(crate) formatting: FormattingClientCapabilities,
    #[serde(rename = "publishDiagnostics")]
    pub(crate) publish_diagnostics: PublishDiagnosticsClientCapabilities,
    #[serde(rename = "codeAction")]
    pub(crate) code_action: CodeActionClientCapabilities,
}

#[derive(Debug, Serialize)]
pub(crate) struct TextDocumentSyncClientCapabilities {
    #[serde(rename = "dynamicRegistration")]
    pub(crate) dynamic_registration: bool,
    #[serde(rename = "willSave")]
    pub(crate) will_save: bool,
    #[serde(rename = "didSave")]
    pub(crate) did_save: bool,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct HoverClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(crate) struct DefinitionClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(crate) struct ReferencesClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(crate) struct RenameClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(crate) struct FormattingClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(crate) struct PublishDiagnosticsClientCapabilities {}

#[derive(Debug, Serialize)]
#[allow(clippy::struct_field_names)]
pub(crate) struct CodeActionClientCapabilities {
    #[serde(rename = "codeActionLiteralSupport")]
    pub(crate) code_action_literal_support: CodeActionLiteralSupport,
    #[serde(rename = "resolveSupport")]
    pub(crate) resolve_support: CodeActionResolveSupport,
    #[serde(rename = "dataSupport")]
    pub(crate) data_support: bool,
    #[serde(rename = "isPreferredSupport")]
    pub(crate) is_preferred_support: bool,
}

impl Default for CodeActionClientCapabilities {
    fn default() -> Self {
        Self {
            code_action_literal_support: CodeActionLiteralSupport::default(),
            resolve_support: CodeActionResolveSupport {
                properties: vec!["edit".into()],
            },
            data_support: true,
            is_preferred_support: true,
        }
    }
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct CodeActionLiteralSupport {
    #[serde(rename = "codeActionKind")]
    pub(crate) code_action_kind: CodeActionKindSet,
}

#[derive(Debug, Serialize)]
pub(crate) struct CodeActionKindSet {
    #[serde(rename = "valueSet")]
    pub(crate) value_set: Vec<String>,
}

impl Default for CodeActionKindSet {
    fn default() -> Self {
        Self {
            value_set: vec![
                String::new(),
                "quickfix".into(),
                "refactor".into(),
                "refactor.extract".into(),
                "refactor.inline".into(),
                "refactor.rewrite".into(),
                "source".into(),
                "source.organizeImports".into(),
                "source.fixAll".into(),
            ],
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CodeActionResolveSupport {
    pub(crate) properties: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DidOpenTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct TextDocumentItem {
    pub(crate) uri: String,
    #[serde(rename = "languageId")]
    pub(crate) language_id: String,
    pub(crate) version: i64,
    pub(crate) text: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DidChangeTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: VersionedTextDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub(crate) content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Serialize)]
pub(crate) struct VersionedTextDocumentIdentifier {
    pub(crate) uri: String,
    pub(crate) version: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TextDocumentContentChangeEvent {
    /// Present for an incremental change (replace just this range); omitted
    /// for a full-document change, where `text` is the entire new content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) range: Option<LspRange>,
    pub(crate) text: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DidSaveTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DidCloseTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
}

#[derive(Debug, Serialize, Clone)]
pub(crate) struct TextDocumentIdentifier {
    pub(crate) uri: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TextDocumentPositionParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
    pub(crate) position: LspPosition,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReferenceParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
    pub(crate) position: LspPosition,
    pub(crate) context: ReferenceContext,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReferenceContext {
    #[serde(rename = "includeDeclaration")]
    pub(crate) include_declaration: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct RenameParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
    pub(crate) position: LspPosition,
    #[serde(rename = "newName")]
    pub(crate) new_name: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DocumentFormattingParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
    pub(crate) options: FormattingOptions,
}

#[derive(Debug, Serialize)]
pub(crate) struct FormattingOptions {
    #[serde(rename = "tabSize")]
    pub(crate) tab_size: u32,
    #[serde(rename = "insertSpaces")]
    pub(crate) insert_spaces: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct CodeActionParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
    pub(crate) range: LspRange,
    pub(crate) context: CodeActionContext,
}

#[derive(Debug, Serialize)]
pub(crate) struct CodeActionContext {
    pub(crate) diagnostics: Vec<LspDiagnostic>,
    #[serde(rename = "triggerKind")]
    pub(crate) trigger_kind: u32,
}

/// Percent-encode a URI path: everything except RFC 3986 unreserved chars
/// and `/` (Windows drive colons stay bare, matching what servers echo).
fn percent_encode_path(path: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        let keep =
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/' | b':');
        if keep {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[must_use]
pub(crate) fn path_to_uri(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();
    if cfg!(windows) {
        // Strip the extended-length path prefix std::fs::canonicalize adds on Windows
        // (\\?\C:\... or \\?\UNC\...), so we emit file:///c:/... not file:////C:/...
        let stripped: std::borrow::Cow<str> = path_str
            .strip_prefix(r"\\?\UNC\")
            .map(|s| format!("//{s}").into())
            .or_else(|| path_str.strip_prefix(r"\\?\").map(Into::into))
            .unwrap_or_else(|| path_str.clone());
        let mut normalized = stripped.replace('\\', "/");
        // Only the drive letter is case-folded: servers key their file
        // graph by the on-disk spelling of the rest of the path.
        if normalized.as_bytes().get(1) == Some(&b':') {
            if let Some(drive) = normalized.get_mut(..1) {
                drive.make_ascii_lowercase();
            }
        }
        let encoded = percent_encode_path(&normalized);
        if encoded.starts_with('/') {
            format!("file://{encoded}")
        } else {
            format!("file:///{encoded}")
        }
    } else {
        format!("file://{}", percent_encode_path(&path_str))
    }
}

/// Canonical map-key form of a file URI: percent-decoded so `c%3A` and `c:`
/// agree, and case-folded on Windows where the filesystem is case-insensitive.
#[must_use]
pub(crate) fn normalize_uri(uri: &str) -> String {
    let decoded = if uri.contains('%') {
        std::borrow::Cow::Owned(percent_decode(uri))
    } else {
        std::borrow::Cow::Borrowed(uri)
    };
    if cfg!(windows) {
        decoded.to_lowercase()
    } else {
        decoded.into_owned()
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[must_use]
pub(crate) fn uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let path = percent_decode(path);
    let path = if cfg!(windows) {
        // `file:///c:/x` -> `c:\x`; `file:////server/share` (UNC) keeps both slashes.
        let p = if path.starts_with("//") {
            path.as_str()
        } else {
            path.strip_prefix('/').unwrap_or(&path)
        };
        p.replace('/', "\\")
    } else {
        path
    };
    Some(std::path::PathBuf::from(path))
}
