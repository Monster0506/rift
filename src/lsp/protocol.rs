use serde::{Deserialize, Serialize};
use serde_json::Value;

/// UTF-16 code units needed to encode `ch` (1, or 2 for chars outside the BMP).
fn utf16_len(ch: char) -> u32 {
    if (ch as u32) > 0xFFFF {
        2
    } else {
        1
    }
}

/// Convert a code-point offset within a line to a UTF-16 code-unit offset —
/// the unit LSP `Position.character` is measured in over the wire.
pub fn char_offset_to_utf16(line: impl Iterator<Item = char>, char_offset: usize) -> u32 {
    line.take(char_offset).map(utf16_len).sum()
}

/// Convert a UTF-16 code-unit offset (as received from an LSP server) to a
/// code-point offset within a line.
pub fn utf16_offset_to_char(line: impl Iterator<Item = char>, utf16_offset: u32) -> usize {
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

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcMessage {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: Option<String>,
    pub params: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    #[allow(dead_code)]
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LspLocation {
    pub uri: String,
    pub range: LspRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDiagnostic {
    pub range: LspRange,
    #[serde(default)]
    pub severity: Option<u32>,
    pub message: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LspTextEdit {
    pub range: LspRange,
    #[serde(rename = "newText")]
    pub new_text: String,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceFolder {
    pub uri: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct InitializeParams {
    #[serde(rename = "processId")]
    pub process_id: Option<u32>,
    #[serde(rename = "rootUri")]
    pub root_uri: Option<String>,
    #[serde(rename = "workspaceFolders")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_folders: Option<Vec<WorkspaceFolder>>,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "initializationOptions")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initialization_options: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct ClientCapabilities {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentClientCapabilities,
    pub general: GeneralCapabilities,
    pub window: WindowCapabilities,
}

#[derive(Debug, Serialize)]
pub struct WindowCapabilities {
    #[serde(rename = "workDoneProgress")]
    pub work_done_progress: bool,
}

#[derive(Debug, Serialize)]
pub struct GeneralCapabilities {
    #[serde(rename = "positionEncodings")]
    pub position_encodings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TextDocumentClientCapabilities {
    pub synchronization: TextDocumentSyncClientCapabilities,
    pub hover: HoverClientCapabilities,
    pub definition: DefinitionClientCapabilities,
    pub references: ReferencesClientCapabilities,
    pub rename: RenameClientCapabilities,
    pub formatting: FormattingClientCapabilities,
    #[serde(rename = "publishDiagnostics")]
    pub publish_diagnostics: PublishDiagnosticsClientCapabilities,
    #[serde(rename = "codeAction")]
    pub code_action: CodeActionClientCapabilities,
}

#[derive(Debug, Serialize)]
pub struct TextDocumentSyncClientCapabilities {
    #[serde(rename = "dynamicRegistration")]
    pub dynamic_registration: bool,
    #[serde(rename = "willSave")]
    pub will_save: bool,
    #[serde(rename = "didSave")]
    pub did_save: bool,
}

#[derive(Debug, Serialize, Default)]
pub struct HoverClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub struct DefinitionClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub struct ReferencesClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub struct RenameClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub struct FormattingClientCapabilities {}

#[derive(Debug, Serialize, Default)]
pub struct PublishDiagnosticsClientCapabilities {}

#[derive(Debug, Serialize)]
pub struct CodeActionClientCapabilities {
    #[serde(rename = "codeActionLiteralSupport")]
    pub code_action_literal_support: CodeActionLiteralSupport,
    #[serde(rename = "resolveSupport")]
    pub resolve_support: CodeActionResolveSupport,
    #[serde(rename = "dataSupport")]
    pub data_support: bool,
    #[serde(rename = "isPreferredSupport")]
    pub is_preferred_support: bool,
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
pub struct CodeActionLiteralSupport {
    #[serde(rename = "codeActionKind")]
    pub code_action_kind: CodeActionKindSet,
}

#[derive(Debug, Serialize)]
pub struct CodeActionKindSet {
    #[serde(rename = "valueSet")]
    pub value_set: Vec<String>,
}

impl Default for CodeActionKindSet {
    fn default() -> Self {
        Self {
            value_set: vec![
                "".into(),
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
pub struct CodeActionResolveSupport {
    pub properties: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DidOpenTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Serialize)]
pub struct TextDocumentItem {
    pub uri: String,
    #[serde(rename = "languageId")]
    pub language_id: String,
    pub version: i64,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct DidChangeTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: VersionedTextDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Serialize)]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
    pub version: i64,
}

#[derive(Debug, Serialize)]
pub struct TextDocumentContentChangeEvent {
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct DidSaveTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DidCloseTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Serialize, Clone)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

#[derive(Debug, Serialize)]
pub struct TextDocumentPositionParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: LspPosition,
}

#[derive(Debug, Serialize)]
pub struct ReferenceParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: LspPosition,
    pub context: ReferenceContext,
}

#[derive(Debug, Serialize)]
pub struct ReferenceContext {
    #[serde(rename = "includeDeclaration")]
    pub include_declaration: bool,
}

#[derive(Debug, Serialize)]
pub struct RenameParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: LspPosition,
    #[serde(rename = "newName")]
    pub new_name: String,
}

#[derive(Debug, Serialize)]
pub struct DocumentFormattingParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub options: FormattingOptions,
}

#[derive(Debug, Serialize)]
pub struct FormattingOptions {
    #[serde(rename = "tabSize")]
    pub tab_size: u32,
    #[serde(rename = "insertSpaces")]
    pub insert_spaces: bool,
}

#[derive(Debug, Serialize)]
pub struct CodeActionParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub range: LspRange,
    pub context: CodeActionContext,
}

#[derive(Debug, Serialize)]
pub struct CodeActionContext {
    pub diagnostics: Vec<LspDiagnostic>,
    #[serde(rename = "triggerKind")]
    pub trigger_kind: u32,
}

pub fn path_to_uri(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();
    if cfg!(windows) {
        // Strip the extended-length path prefix that std::fs::canonicalize adds on
        // Windows (\\?\C:\... or \\?\UNC\server\share), so we always emit a clean
        // file:///c:/... URI instead of file:////C:/...
        let stripped: std::borrow::Cow<str> = if let Some(s) = path_str.strip_prefix(r"\\?\UNC\") {
            format!("//{}", s).into()
        } else if let Some(s) = path_str.strip_prefix(r"\\?\") {
            s.into()
        } else {
            path_str
        };
        let normalized = stripped.replace('\\', "/");
        let uri = if normalized.starts_with('/') {
            format!("file://{}", normalized)
        } else {
            format!("file:///{}", normalized)
        };
        // Lowercase so our URIs always match what the LSP server sends,
        // which uses lowercase drive letters (file:///c:/...) on Windows.
        uri.to_lowercase()
    } else {
        format!("file://{}", path_str)
    }
}

/// Normalize a file URI for use as a HashMap key.
/// On Windows, drive letters are case-insensitive so `file:///C:/` and
/// `file:///c:/` must resolve to the same key.
pub fn normalize_uri(uri: &str) -> String {
    if cfg!(windows) {
        uri.to_lowercase()
    } else {
        uri.to_string()
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

pub fn uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let path = percent_decode(path);
    let path = if cfg!(windows) {
        path.strip_prefix('/').unwrap_or(&path).replace('/', "\\")
    } else {
        path.to_string()
    };
    Some(std::path::PathBuf::from(path))
}
