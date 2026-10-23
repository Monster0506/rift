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
pub(super) fn char_offset_to_utf16(line: impl Iterator<Item = char>, char_offset: usize) -> u32 {
    line.take(char_offset).map(utf16_len).sum()
}

/// Convert a UTF-16 code-unit offset (as received from an LSP server) to a
/// code-point offset within a line.
pub(super) fn utf16_offset_to_char(line: impl Iterator<Item = char>, utf16_offset: u32) -> usize {
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
pub(super) fn char_offset_to_utf8(line: impl Iterator<Item = char>, char_offset: usize) -> u32 {
    line.take(char_offset)
        .map(|c| u32::try_from(c.len_utf8()).unwrap_or(u32::MAX))
        .sum()
}

/// Convert a UTF-8 byte offset (as received from an LSP server) to a
/// code-point offset within a line.
pub(super) fn utf8_offset_to_char(line: impl Iterator<Item = char>, byte_offset: u32) -> usize {
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
    pub(super) fn from_wire(s: &str) -> Option<Self> {
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
pub(super) struct JsonRpcRequest {
    pub(super) jsonrpc: &'static str,
    pub(super) id: u64,
    pub(super) method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct JsonRpcNotification {
    pub(super) jsonrpc: &'static str,
    pub(super) method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) params: Option<Value>,
}

/// A JSON-RPC request/response id, which the spec permits to be either a
/// number or a string. Server-initiated requests may use either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub(super) enum RequestId {
    Number(u64),
    String(String),
}

#[derive(Debug, Deserialize)]
pub(super) struct JsonRpcMessage {
    pub(super) id: Option<Value>,
    pub(super) method: Option<String>,
    pub(super) params: Option<Value>,
    pub(super) result: Option<Value>,
    pub(super) error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub(super) struct JsonRpcError {
    pub(super) message: String,
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
struct LspLocationLink {
    #[serde(rename = "targetUri")]
    uri: String,
    #[serde(rename = "targetRange")]
    range: LspRange,
    #[serde(rename = "targetSelectionRange")]
    selection_range: Option<LspRange>,
}

impl From<LspLocationLink> for LspLocation {
    fn from(link: LspLocationLink) -> Self {
        Self {
            uri: link.uri,
            range: link.selection_range.unwrap_or(link.range),
        }
    }
}

/// Parse one `Location | LocationLink` value.
pub(super) fn location_from_value(v: Value) -> Option<LspLocation> {
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
pub(super) struct WorkspaceFolder {
    pub(super) uri: String,
    pub(super) name: String,
}

#[derive(Debug, Serialize)]
pub(super) struct InitializeParams {
    #[serde(rename = "processId")]
    pub(super) process_id: Option<u32>,
    #[serde(rename = "rootUri")]
    pub(super) root_uri: Option<String>,
    #[serde(rename = "workspaceFolders")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) workspace_folders: Option<Vec<WorkspaceFolder>>,
    pub(super) capabilities: ClientCapabilities,
    #[serde(rename = "initializationOptions")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) initialization_options: Option<Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct ClientCapabilities {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentClientCapabilities,
    pub(super) workspace: WorkspaceClientCapabilities,
    pub(super) general: GeneralCapabilities,
    pub(super) window: WindowCapabilities,
}

#[derive(Debug, Serialize)]
pub(super) struct WorkspaceClientCapabilities {
    /// We answer `workspace/applyEdit` by applying the edit in the editor.
    #[serde(rename = "applyEdit")]
    pub(super) apply_edit: bool,
    #[serde(rename = "workspaceEdit")]
    pub(super) workspace_edit: WorkspaceEditClientCapabilities,
}

#[derive(Debug, Serialize)]
pub(super) struct WorkspaceEditClientCapabilities {
    #[serde(rename = "documentChanges")]
    pub(super) document_changes: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct WindowCapabilities {
    #[serde(rename = "workDoneProgress")]
    pub(super) work_done_progress: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct GeneralCapabilities {
    #[serde(rename = "positionEncodings")]
    pub(super) position_encodings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct TextDocumentClientCapabilities {
    pub(super) synchronization: TextDocumentSyncClientCapabilities,
    pub(super) hover: HoverClientCapabilities,
    pub(super) definition: DefinitionClientCapabilities,
    pub(super) references: ReferencesClientCapabilities,
    pub(super) rename: RenameClientCapabilities,
    pub(super) formatting: FormattingClientCapabilities,
    #[serde(rename = "publishDiagnostics")]
    pub(super) publish_diagnostics: PublishDiagnosticsClientCapabilities,
    #[serde(rename = "codeAction")]
    pub(super) code_action: CodeActionClientCapabilities,
}

#[derive(Debug, Serialize)]
pub(super) struct TextDocumentSyncClientCapabilities {
    #[serde(rename = "dynamicRegistration")]
    pub(super) dynamic_registration: bool,
    #[serde(rename = "willSave")]
    pub(super) will_save: bool,
    #[serde(rename = "didSave")]
    pub(super) did_save: bool,
}

#[derive(Debug, Serialize, Default)]
pub(super) struct HoverClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(super) struct DefinitionClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(super) struct ReferencesClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(super) struct RenameClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(super) struct FormattingClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub(super) struct PublishDiagnosticsClientCapabilities {}

#[derive(Debug, Serialize)]
pub(super) struct CodeActionClientCapabilities {
    #[serde(rename = "codeActionLiteralSupport")]
    literal: CodeActionLiteralSupport,
    #[serde(rename = "resolveSupport")]
    resolve: CodeActionResolveSupport,
    #[serde(rename = "dataSupport")]
    data: bool,
    #[serde(rename = "isPreferredSupport")]
    is_preferred: bool,
}

impl Default for CodeActionClientCapabilities {
    fn default() -> Self {
        Self {
            literal: CodeActionLiteralSupport::default(),
            resolve: CodeActionResolveSupport {
                properties: vec!["edit".into()],
            },
            data: true,
            is_preferred: true,
        }
    }
}

#[derive(Debug, Serialize, Default)]
struct CodeActionLiteralSupport {
    #[serde(rename = "codeActionKind")]
    code_action_kind: CodeActionKindSet,
}

#[derive(Debug, Serialize)]
struct CodeActionKindSet {
    #[serde(rename = "valueSet")]
    value_set: Vec<String>,
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
struct CodeActionResolveSupport {
    properties: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct DidOpenTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentItem,
}

#[derive(Debug, Serialize)]
pub(super) struct TextDocumentItem {
    pub(super) uri: String,
    #[serde(rename = "languageId")]
    pub(super) language_id: String,
    pub(super) version: i64,
    pub(super) text: String,
}

#[derive(Debug, Serialize)]
pub(super) struct DidChangeTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: VersionedTextDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub(super) content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Serialize)]
pub(super) struct VersionedTextDocumentIdentifier {
    pub(super) uri: String,
    pub(super) version: i64,
}

#[derive(Debug, Serialize)]
pub(super) struct TextDocumentContentChangeEvent {
    /// Present for an incremental change (replace just this range); omitted
    /// for a full-document change, where `text` is the entire new content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) range: Option<LspRange>,
    pub(super) text: String,
}

#[derive(Debug, Serialize)]
pub(super) struct DidSaveTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentIdentifier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) text: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct DidCloseTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentIdentifier,
}

#[derive(Debug, Serialize, Clone)]
pub(super) struct TextDocumentIdentifier {
    pub(super) uri: String,
}

#[derive(Debug, Serialize)]
pub(super) struct TextDocumentPositionParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentIdentifier,
    pub(super) position: LspPosition,
}

#[derive(Debug, Serialize)]
pub(super) struct ReferenceParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentIdentifier,
    pub(super) position: LspPosition,
    pub(super) context: ReferenceContext,
}

#[derive(Debug, Serialize)]
pub(super) struct ReferenceContext {
    #[serde(rename = "includeDeclaration")]
    pub(super) include_declaration: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct RenameParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentIdentifier,
    pub(super) position: LspPosition,
    #[serde(rename = "newName")]
    pub(super) new_name: String,
}

#[derive(Debug, Serialize)]
pub(super) struct DocumentFormattingParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentIdentifier,
    pub(super) options: FormattingOptions,
}

#[derive(Debug, Serialize)]
pub(super) struct FormattingOptions {
    #[serde(rename = "tabSize")]
    pub(super) tab_size: u32,
    #[serde(rename = "insertSpaces")]
    pub(super) insert_spaces: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct CodeActionParams {
    #[serde(rename = "textDocument")]
    pub(super) text_document: TextDocumentIdentifier,
    pub(super) range: LspRange,
    pub(super) context: CodeActionContext,
}

#[derive(Debug, Serialize)]
pub(super) struct CodeActionContext {
    pub(super) diagnostics: Vec<LspDiagnostic>,
    #[serde(rename = "triggerKind")]
    pub(super) trigger_kind: u32,
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
