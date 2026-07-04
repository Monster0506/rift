pub mod client;
pub mod config;
pub mod protocol;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use client::{LspClient, RawLspMessage};
use protocol::{
    path_to_uri, ClientCapabilities, CodeActionContext, CodeActionParams,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentFormattingParams, FormattingOptions, GeneralCapabilities,
    InitializeParams, LspPosition, LspRange, ReferenceContext, ReferenceParams, RenameParams,
    TextDocumentClientCapabilities, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams, TextDocumentSyncClientCapabilities,
    VersionedTextDocumentIdentifier, WindowCapabilities,
};
use serde_json::Value;

/// A processed LSP message delivered to the editor.
#[derive(Debug)]
pub enum LspMessage {
    Diagnostics {
        uri: String,
        diagnostics: Vec<protocol::LspDiagnostic>,
    },
    GotoDefinitionResult {
        locations: Vec<protocol::LspLocation>,
    },
    ReferencesResult {
        locations: Vec<protocol::LspLocation>,
    },
    HoverResult {
        contents: String,
    },
    RenameResult {
        workspace_edit: Value,
    },
    FormattingResult {
        uri: String,
        edits: Vec<protocol::LspTextEdit>,
    },
    CodeActionResult {
        /// Full action objects (title, edit, command) from the server.
        actions: Vec<serde_json::Value>,
    },
    /// Result of a codeAction/resolve — the same action but with edit populated.
    CodeActionResolved {
        action: serde_json::Value,
    },
    Error {
        method: String,
        message: String,
    },
    Log {
        message: String,
    },
    /// Emitted once after a language server's initialize handshake completes.
    ServerConnected {
        language: String,
        server_name: String,
    },
    /// Emitted when all $/progress indexing tokens for a server have finished.
    ServerReady {
        language: String,
    },
    /// A $/progress update from the server (begin, report, or end).
    Progress {
        language: String,
        message: String,
    },
}

/// Tracks per-document state sent to a language server.
#[derive(Debug)]
struct DocState {
    language: String,
    version: i64,
}

/// Metadata for an in-flight LSP request.
#[derive(Debug)]
struct PendingRequest {
    method: String,
    /// For requests that need a URI in their response (e.g. formatting), stored
    /// here so it doesn't have to be encoded into the method string.
    uri: Option<String>,
}

/// The overall LSP integration layer. Owns one LspClient per active language server.
pub struct LspManager {
    /// language name -> active client
    clients: HashMap<String, LspClient>,
    /// document URI -> DocState
    open_docs: HashMap<String, DocState>,
    /// request id -> metadata for cross-client routing
    pending_requests: HashMap<u64, PendingRequest>,
    /// project root, if any
    workspace_root: Option<PathBuf>,
    /// Plugin-registered server configs: language -> config
    registered_servers: HashMap<String, config::LspServerConfig>,
    /// language -> count of active $/progress tokens (indexing in progress when > 0)
    indexing_tokens: HashMap<String, usize>,
    /// language -> total begin events seen (for X/Y progress display)
    indexing_started: HashMap<String, usize>,
    /// language -> total end events seen
    indexing_ended: HashMap<String, usize>,
    /// language -> human-readable server name (e.g. "rust-analyzer"), set on initialize
    server_names: HashMap<String, String>,
    /// When indexing_tokens last dropped to 0. ServerReady is emitted after a 600ms grace
    /// period so that tokens arriving in rapid succession (like rust-analyzer's quick
    /// initial Fetching → Building CrateGraph sequence) don't trigger premature readiness.
    indexing_idle_since: HashMap<String, std::time::Instant>,
    /// Documents queued for didOpen before their server's initialize handshake completes.
    /// language -> list of (uri, params) waiting to be sent.
    pending_opens: HashMap<String, Vec<(String, serde_json::Value)>>,
    /// Debug log messages queued by internal methods that can't return LspMessage directly.
    pending_logs: Vec<String>,
    /// language -> negotiated `Position.character` unit, from the server's
    /// initialize response (LSP defaults to UTF-16 when it omits the field).
    position_encodings: HashMap<String, crate::lsp::protocol::PositionEncoding>,
}

impl LspManager {
    /// Returns true if the language server for the given language is still indexing
    /// (active tokens OR within the 600ms grace period after all tokens ended).
    pub fn is_indexing(&self, language: &str) -> bool {
        if self.indexing_tokens.get(language).copied().unwrap_or(0) > 0 {
            return true;
        }
        self.indexing_idle_since.contains_key(language)
    }

    /// Returns true if the server for the given file path is still indexing.
    pub fn is_indexing_path(&self, path: &Path) -> bool {
        let uri = path_to_uri(path);
        self.open_docs
            .get(&uri)
            .map(|s| self.is_indexing(&s.language))
            .unwrap_or(false)
    }
}

impl LspManager {
    pub fn new(workspace_root: Option<PathBuf>) -> Self {
        Self {
            clients: HashMap::new(),
            open_docs: HashMap::new(),
            pending_requests: HashMap::new(),
            workspace_root,
            registered_servers: HashMap::new(),
            indexing_tokens: HashMap::new(),
            indexing_started: HashMap::new(),
            indexing_ended: HashMap::new(),
            server_names: HashMap::new(),
            indexing_idle_since: HashMap::new(),
            pending_opens: HashMap::new(),
            pending_logs: Vec::new(),
            position_encodings: HashMap::new(),
        }
    }

    /// Negotiated `Position.character` unit for the server handling `path`
    /// (UTF-16 if unknown/not yet negotiated, per the LSP default).
    pub fn position_encoding_for_path(
        &self,
        path: &Path,
    ) -> crate::lsp::protocol::PositionEncoding {
        let uri = path_to_uri(path);
        self.open_docs
            .get(&uri)
            .and_then(|s| self.position_encodings.get(&s.language))
            .copied()
            .unwrap_or_default()
    }

    /// Returns the LSP language name for a given document URI, if the document is open.
    pub fn language_for_uri(&self, uri: &str) -> Option<&str> {
        self.open_docs.get(uri).map(|s| s.language.as_str())
    }

    /// Returns the human-readable server name for a language, if the server has connected.
    pub fn server_name(&self, lang: &str) -> Option<&str> {
        self.server_names.get(lang).map(|s| s.as_str())
    }

    /// Returns (ended, started) for X/Y progress display, or None if no indexing has started.
    pub fn indexing_progress(&self, lang: &str) -> Option<(usize, usize)> {
        let started = self.indexing_started.get(lang).copied().unwrap_or(0);
        if started == 0 {
            return None;
        }
        let ended = self.indexing_ended.get(lang).copied().unwrap_or(0);
        Some((ended, started))
    }

    /// Register a language server (called by plugins via `rift.lsp.register()`).
    pub fn register_server(&mut self, language: String, server: config::LspServerConfig) {
        self.registered_servers.insert(language, server);
    }

    /// Walk up from `file` to find the nearest project root using the given markers.
    fn find_workspace_root(file: &Path, markers: &[String]) -> Option<PathBuf> {
        let mut dir = file.parent()?;
        loop {
            for marker in markers {
                if dir.join(marker).exists() {
                    return Some(dir.to_path_buf());
                }
            }
            match dir.parent() {
                Some(p) => dir = p,
                None => return None,
            }
        }
    }

    /// Get or start the LSP client for a given language. Returns `None` if no
    /// server is registered for that language.
    ///
    /// `file_hint` is the path of the file being opened; it is used to locate
    /// the nearest project root (Cargo.toml / .git / …) so the server receives
    /// an accurate `rootUri` on first start.
    fn ensure_client(
        &mut self,
        language: &str,
        file_hint: Option<&Path>,
    ) -> Option<&mut LspClient> {
        if !self.clients.contains_key(language) {
            let server = self.registered_servers.get(language)?.clone();

            // Prefer the project root nearest to the opened file; fall back to
            // the global workspace root (cwd at launch).
            let root_uri = file_hint
                .and_then(|f| {
                    self.pending_logs.push(format!(
                        "LSP [{}]: file_hint={}",
                        language,
                        f.display()
                    ));
                    Self::find_workspace_root(f, &server.root_markers)
                })
                .or_else(|| self.workspace_root.clone())
                .as_ref()
                .map(|p| protocol::path_to_uri(p));
            self.pending_logs.push(format!(
                "LSP [{}]: rootUri={}",
                language,
                root_uri.as_deref().unwrap_or("null")
            ));

            let mut client = LspClient::start(
                language.to_string(),
                &server.command,
                &server.args,
                root_uri.clone(),
            )
            .ok()?;

            // Send initialize
            let workspace_folders = root_uri.as_ref().map(|uri| {
                let name = uri.rsplit('/').next().unwrap_or("workspace").to_string();
                vec![protocol::WorkspaceFolder {
                    uri: uri.clone(),
                    name,
                }]
            });
            let params = serde_json::to_value(InitializeParams {
                process_id: Some(std::process::id()),
                root_uri,
                workspace_folders,
                capabilities: ClientCapabilities {
                    general: GeneralCapabilities {
                        position_encodings: vec!["utf-16".into(), "utf-8".into()],
                    },
                    text_document: TextDocumentClientCapabilities {
                        synchronization: TextDocumentSyncClientCapabilities {
                            dynamic_registration: false,
                            will_save: false,
                            did_save: true,
                        },
                        hover: Default::default(),
                        definition: Default::default(),
                        references: Default::default(),
                        rename: Default::default(),
                        formatting: Default::default(),
                        publish_diagnostics: Default::default(),
                        code_action: Default::default(),
                    },
                    window: WindowCapabilities {
                        work_done_progress: true,
                    },
                },
                initialization_options: server.initialization_options.clone(),
            })
            .ok()?;

            let req_id = client.send_request("initialize", params);
            self.pending_requests.insert(
                req_id,
                PendingRequest {
                    method: "initialize".to_string(),
                    uri: None,
                },
            );

            self.clients.insert(language.to_string(), client);
        }

        self.clients.get_mut(language)
    }

    /// Notify the server that a document was opened.
    pub fn did_open(&mut self, path: &Path, language: &str, content: &str) {
        let uri = path_to_uri(path);

        if self.open_docs.contains_key(&uri) {
            return;
        }

        self.open_docs.insert(
            uri.clone(),
            DocState {
                language: language.to_string(),
                version: 1,
            },
        );

        // Ensure the client exists (spawns and sends initialize if needed).
        if self.ensure_client(language, Some(path)).is_none() {
            return;
        };

        let params = serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: config::language_id(language).to_string(),
                version: 1,
                text: content.to_string(),
            },
        })
        .unwrap_or(Value::Null);

        // Only send didOpen after the initialize handshake is complete.
        // If not yet initialized, queue it — poll() will flush it when ready.
        let initialized = self
            .clients
            .get(language)
            .map(|c| c.initialized)
            .unwrap_or(false);
        if initialized {
            if let Some(client) = self.clients.get_mut(language) {
                client.send_notification("textDocument/didOpen", params);
            }
        } else {
            self.pending_opens
                .entry(language.to_string())
                .or_default()
                .push((uri.clone(), params));
        }
    }

    /// True when a did_change for `path` would actually reach a live client,
    /// so callers can skip materializing the document content otherwise.
    pub fn is_tracking(&self, path: &Path) -> bool {
        let uri = path_to_uri(path);
        self.open_docs
            .get(&uri)
            .is_some_and(|state| self.clients.contains_key(&state.language))
    }

    /// Notify the server that a document's content changed.
    pub fn did_change(&mut self, path: &Path, content: &str) {
        let uri = path_to_uri(path);

        let (language, version) = match self.open_docs.get_mut(&uri) {
            Some(state) => {
                state.version += 1;
                (state.language.clone(), state.version)
            }
            None => return,
        };

        let client = match self.clients.get_mut(&language) {
            Some(c) => c,
            None => return,
        };

        let params = serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri, version },
            content_changes: vec![TextDocumentContentChangeEvent {
                text: content.to_string(),
            }],
        })
        .unwrap_or(Value::Null);

        client.send_notification("textDocument/didChange", params);
    }

    /// Notify the server that a document was saved.
    pub fn did_save(&mut self, path: &Path, content: Option<&str>) {
        let uri = path_to_uri(path);
        let language = match self.open_docs.get(&uri) {
            Some(s) => s.language.clone(),
            None => return,
        };
        let client = match self.clients.get_mut(&language) {
            Some(c) => c,
            None => return,
        };

        let params = serde_json::to_value(DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
            text: content.map(|s| s.to_string()),
        })
        .unwrap_or(Value::Null);

        client.send_notification("textDocument/didSave", params);
    }

    /// Notify the server that a document was closed.
    pub fn did_close(&mut self, path: &Path) {
        let uri = path_to_uri(path);
        let state = match self.open_docs.remove(&uri) {
            Some(s) => s,
            None => return,
        };
        let client = match self.clients.get_mut(&state.language) {
            Some(c) => c,
            None => return,
        };

        let params = serde_json::to_value(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
        })
        .unwrap_or(Value::Null);

        client.send_notification("textDocument/didClose", params);
    }

    /// Request goto definition. Returns the request id, or None if not available.
    pub fn goto_definition(&mut self, path: &Path, line: u32, col: u32) -> Option<u64> {
        let uri = path_to_uri(path);
        let language = self.open_docs.get(&uri)?.language.clone();
        let client = self.clients.get_mut(&language)?;

        let params = serde_json::to_value(TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: LspPosition {
                line,
                character: col,
            },
        })
        .ok()?;

        let req_id = client.send_request("textDocument/definition", params);
        self.pending_requests.insert(
            req_id,
            PendingRequest {
                method: "textDocument/definition".to_string(),
                uri: None,
            },
        );
        Some(req_id)
    }

    /// Request references.
    pub fn references(&mut self, path: &Path, line: u32, col: u32) -> Option<u64> {
        let uri = path_to_uri(path);
        let language = self.open_docs.get(&uri)?.language.clone();
        let client = self.clients.get_mut(&language)?;

        let params = serde_json::to_value(ReferenceParams {
            text_document: TextDocumentIdentifier { uri },
            position: LspPosition {
                line,
                character: col,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
        })
        .ok()?;

        let req_id = client.send_request("textDocument/references", params);
        self.pending_requests.insert(
            req_id,
            PendingRequest {
                method: "textDocument/references".to_string(),
                uri: None,
            },
        );
        Some(req_id)
    }

    /// Request hover.
    pub fn hover(&mut self, path: &Path, line: u32, col: u32) -> Option<u64> {
        let uri = path_to_uri(path);
        let language = self.open_docs.get(&uri)?.language.clone();
        let client = self.clients.get_mut(&language)?;

        let params = serde_json::to_value(TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: LspPosition {
                line,
                character: col,
            },
        })
        .ok()?;

        let req_id = client.send_request("textDocument/hover", params);
        self.pending_requests.insert(
            req_id,
            PendingRequest {
                method: "textDocument/hover".to_string(),
                uri: None,
            },
        );
        Some(req_id)
    }

    /// Request rename.
    pub fn rename(&mut self, path: &Path, line: u32, col: u32, new_name: String) -> Option<u64> {
        let uri = path_to_uri(path);
        let language = self.open_docs.get(&uri)?.language.clone();
        let client = self.clients.get_mut(&language)?;

        let params = serde_json::to_value(RenameParams {
            text_document: TextDocumentIdentifier { uri },
            position: LspPosition {
                line,
                character: col,
            },
            new_name,
        })
        .ok()?;

        let req_id = client.send_request("textDocument/rename", params);
        self.pending_requests.insert(
            req_id,
            PendingRequest {
                method: "textDocument/rename".to_string(),
                uri: None,
            },
        );
        Some(req_id)
    }

    /// Request document formatting.
    pub fn format(&mut self, path: &Path, tab_size: u32, insert_spaces: bool) -> Option<u64> {
        let uri = path_to_uri(path);
        let language = self.open_docs.get(&uri)?.language.clone();
        let client = self.clients.get_mut(&language)?;

        let params = serde_json::to_value(DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            options: FormattingOptions {
                tab_size,
                insert_spaces,
            },
        })
        .ok()?;

        let req_id = client.send_request("textDocument/formatting", params);
        self.pending_requests.insert(
            req_id,
            PendingRequest {
                method: "textDocument/formatting".to_string(),
                uri: Some(uri),
            },
        );
        Some(req_id)
    }

    /// Request code actions at the cursor position, optionally scoped to specific diagnostics.
    pub fn code_action(
        &mut self,
        path: &Path,
        line: u32,
        col: u32,
        diagnostics: Vec<protocol::LspDiagnostic>,
    ) -> Option<u64> {
        let uri = path_to_uri(path);
        let language = self.open_docs.get(&uri)?.language.clone();
        let client = self.clients.get_mut(&language)?;

        let pos = LspPosition {
            line,
            character: col,
        };
        let params = serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier { uri },
            range: LspRange {
                start: pos.clone(),
                end: LspPosition {
                    line,
                    character: col + 1,
                },
            },
            context: CodeActionContext {
                diagnostics,
                trigger_kind: 1,
            },
        })
        .ok()?;

        let req_id = client.send_request("textDocument/codeAction", params);
        self.pending_requests.insert(
            req_id,
            PendingRequest {
                method: "textDocument/codeAction".to_string(),
                uri: None,
            },
        );
        Some(req_id)
    }

    /// Resolve a code action that was returned without an edit field.
    /// The server fills in the `edit` field and returns the completed action.
    pub fn resolve_code_action(
        &mut self,
        language: &str,
        action: serde_json::Value,
    ) -> Option<u64> {
        let client = self.clients.get_mut(language)?;
        let req_id = client.send_request("codeAction/resolve", action);
        self.pending_requests.insert(
            req_id,
            PendingRequest {
                method: "codeAction/resolve".to_string(),
                uri: None,
            },
        );
        Some(req_id)
    }

    /// Poll all clients and return processed LSP messages.
    pub fn poll(&mut self) -> Vec<LspMessage> {
        let mut results = Vec::new();

        for msg in self.pending_logs.drain(..) {
            results.push(LspMessage::Log { message: msg });
        }

        let languages: Vec<String> = self.clients.keys().cloned().collect();

        for lang in &languages {
            let raw_msgs = if let Some(c) = self.clients.get_mut(lang) {
                c.poll_raw()
            } else {
                continue;
            };

            for raw in raw_msgs {
                match raw {
                    RawLspMessage::Response { id, result } => {
                        let pending = self.pending_requests.remove(&id);
                        let method = pending.as_ref().map(|p| p.method.as_str()).unwrap_or("");
                        let uri = pending.as_ref().and_then(|p| p.uri.as_deref());

                        // Also remove from client's pending map
                        if let Some(c) = self.clients.get_mut(lang) {
                            c.pending.remove(&id);
                        }

                        // Mark initialized after successful initialize
                        if method == "initialize" {
                            results.push(LspMessage::Log {
                                message: format!("LSP [{}]: initialize response received", lang),
                            });
                            if let Some(c) = self.clients.get_mut(lang) {
                                c.initialized = true;
                                c.send_notification("initialized", serde_json::json!({}));
                            }

                            // Server's chosen Position.character unit; LSP
                            // defaults to UTF-16 when the field is absent.
                            let encoding = result
                                .get("capabilities")
                                .and_then(|c| c.get("positionEncoding"))
                                .and_then(|v| v.as_str())
                                .and_then(crate::lsp::protocol::PositionEncoding::from_wire)
                                .unwrap_or_default();
                            self.position_encodings.insert(lang.to_string(), encoding);

                            // Flush any didOpen calls that arrived before initialization.
                            let queued = self.pending_opens.remove(lang).unwrap_or_default();
                            results.push(LspMessage::Log {
                                message: format!(
                                    "LSP [{}]: flushing {} queued didOpen(s)",
                                    lang,
                                    queued.len()
                                ),
                            });
                            for (uri, params) in queued {
                                results.push(LspMessage::Log {
                                    message: format!("LSP [{}]: didOpen -> {}", lang, uri),
                                });
                                if let Some(c) = self.clients.get_mut(lang) {
                                    c.send_notification("textDocument/didOpen", params);
                                }
                            }

                            // Extract the human-readable server name from capabilities,
                            // falling back to the language name if the server omits it.
                            let server_name = result
                                .get("serverInfo")
                                .and_then(|si| si.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or(lang)
                                .to_string();
                            self.server_names
                                .insert(lang.to_string(), server_name.clone());
                            results.push(LspMessage::ServerConnected {
                                language: lang.to_string(),
                                server_name,
                            });
                            // Seed the idle timer so ServerReady fires if no $/progress tokens arrive.
                            self.indexing_idle_since
                                .entry(lang.to_string())
                                .or_insert_with(std::time::Instant::now);
                        } else if let Some(msg) = route_response(method, uri, result.clone()) {
                            results.push(msg);
                        } else if !method.is_empty() {
                            results.push(LspMessage::Log {
                                message: format!(
                                    "LSP [{}]: unrouted response method='{}' result={}",
                                    lang, method, result
                                ),
                            });
                        }
                    }
                    RawLspMessage::ResponseError { id, message } => {
                        let method = self
                            .pending_requests
                            .remove(&id)
                            .map(|p| p.method)
                            .unwrap_or_default();
                        if let Some(c) = self.clients.get_mut(lang) {
                            c.pending.remove(&id);
                        }
                        results.push(LspMessage::Error { method, message });
                    }
                    RawLspMessage::Notification { method, params } => {
                        // Track $/progress tokens; emit ServerReady when they all finish.
                        if method == "$/progress" {
                            let val = params.get("value").cloned().unwrap_or(Value::Null);
                            let kind = val.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                            let title = val.get("title").and_then(|t| t.as_str()).unwrap_or("");
                            let message = val.get("message").and_then(|m| m.as_str()).unwrap_or("");

                            let counter = self.indexing_tokens.entry(lang.clone()).or_insert(0);
                            match kind {
                                "begin" => {
                                    *counter += 1;
                                    // Cancel any pending idle timer — a new token arrived.
                                    self.indexing_idle_since.remove(lang);
                                    *self.indexing_started.entry(lang.clone()).or_insert(0) += 1;
                                    results.push(LspMessage::Progress {
                                        language: lang.to_string(),
                                        message: title.to_string(),
                                    });
                                }
                                "report" => {
                                    results.push(LspMessage::Progress {
                                        language: lang.to_string(),
                                        message: message.to_string(),
                                    });
                                }
                                "end" => {
                                    *counter = counter.saturating_sub(1);
                                    *self.indexing_ended.entry(lang.clone()).or_insert(0) += 1;
                                    results.push(LspMessage::Progress {
                                        language: lang.to_string(),
                                        message: "end".to_string(),
                                    });
                                    // Start the idle timer: ServerReady fires after 600ms of
                                    // no new begin events, guarding against rapid token bursts
                                    // (e.g. rust-analyzer's quick initial Fetching finishing
                                    // before Building CrateGraph has even started).
                                    if *counter == 0 {
                                        self.indexing_idle_since
                                            .entry(lang.clone())
                                            .or_insert_with(std::time::Instant::now);
                                    }
                                }
                                _ => {}
                            }
                        }
                        if let Some(msg) = route_notification(&method, params.clone()) {
                            results.push(msg);
                        } else if method != "$/progress" {
                            results.push(LspMessage::Log {
                                message: format!(
                                    "LSP [{}]: unhandled notification '{}' params={}",
                                    lang, method, params
                                ),
                            });
                        }
                    }
                    RawLspMessage::ServerRequest { id, method: _, .. } => {
                        // Respond to server-initiated requests (e.g. window/workDoneProgress/create).
                        if let Some(c) = self.clients.get_mut(lang) {
                            c.send_response(id, Value::Null);
                        }
                    }
                    RawLspMessage::ParseError { message } => {
                        results.push(LspMessage::Log {
                            message: format!("LSP [{}]: {}", lang, message),
                        });
                    }
                }
            }
        }

        // Emit ServerReady for any language whose indexing counter has been at 0
        // for at least 600ms (grace period to absorb back-to-back token bursts).
        let grace = std::time::Duration::from_millis(600);
        let ready_langs: Vec<String> = self
            .indexing_idle_since
            .iter()
            .filter(|(_, t)| t.elapsed() >= grace)
            .map(|(lang, _)| lang.clone())
            .collect();
        for lang in ready_langs {
            self.indexing_idle_since.remove(&lang);
            results.push(LspMessage::ServerReady { language: lang });
        }

        results
    }

    /// Returns `true` if there is an active LSP client for the given file path.
    pub fn has_client_for_path(&self, path: &Path) -> bool {
        let uri = path_to_uri(path);
        self.open_docs.contains_key(&uri)
    }

    /// Get the list of diagnostics for a path (from last polled diagnostics).
    /// Diagnostics are pushed by the server and handled via `LspMessage::Diagnostics`.
    pub fn shutdown_all(&mut self) {
        for client in self.clients.values_mut() {
            let _ = client.send_request("shutdown", Value::Null);
        }
        for client in self.clients.values_mut() {
            client.send_notification("exit", Value::Null);
        }
        // Brief grace period for servers to exit on `exit`; dropping each
        // client below kills+reaps anything still running.
        std::thread::sleep(std::time::Duration::from_millis(200));
        self.clients.clear();
    }
}

fn route_response(method: &str, uri: Option<&str>, result: Value) -> Option<LspMessage> {
    match method {
        "initialize" => None, // handled separately (send initialized notif)
        "textDocument/definition" | "textDocument/declaration" | "textDocument/typeDefinition" => {
            let locations = parse_locations(result);
            Some(LspMessage::GotoDefinitionResult { locations })
        }
        "textDocument/references" => {
            let locations = parse_locations(result);
            Some(LspMessage::ReferencesResult { locations })
        }
        "textDocument/hover" => {
            let contents = extract_hover_text(&result).unwrap_or_default();
            Some(LspMessage::HoverResult { contents })
        }
        "textDocument/rename" => Some(LspMessage::RenameResult {
            workspace_edit: result,
        }),
        "textDocument/formatting" => {
            let edits = parse_text_edits(result);
            Some(LspMessage::FormattingResult {
                uri: uri.unwrap_or("").to_string(),
                edits,
            })
        }
        "textDocument/codeAction" => {
            let actions = parse_code_actions(result);
            Some(LspMessage::CodeActionResult { actions })
        }
        "codeAction/resolve" => Some(LspMessage::CodeActionResolved { action: result }),
        _ => None,
    }
}

fn route_notification(method: &str, params: Value) -> Option<LspMessage> {
    match method {
        "textDocument/publishDiagnostics" => {
            let uri = params["uri"].as_str()?.to_string();
            let diags: Vec<protocol::LspDiagnostic> = params["diagnostics"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| serde_json::from_value(v.clone()).ok())
                        .collect()
                })
                .unwrap_or_default();
            Some(LspMessage::Diagnostics {
                uri,
                diagnostics: diags,
            })
        }
        _ => None,
    }
}

fn parse_locations(result: Value) -> Vec<protocol::LspLocation> {
    match result {
        Value::Array(arr) => arr
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
        Value::Object(_) => serde_json::from_value(result).ok().into_iter().collect(),
        _ => vec![],
    }
}

fn extract_hover_text(result: &Value) -> Option<String> {
    if result.is_null() {
        return None;
    }
    let contents = &result["contents"];
    // contents can be: string, MarkedString, { kind, value }, or array of those
    if let Some(s) = contents.as_str() {
        return Some(s.to_string());
    }
    if let Some(v) = contents.get("value").and_then(|v| v.as_str()) {
        return Some(v.to_string());
    }
    if let Some(arr) = contents.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                if let Some(s) = item.as_str() {
                    Some(s.to_string())
                } else {
                    item.get("value")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                }
            })
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("\n\n"));
        }
    }
    None
}

fn parse_text_edits(result: Value) -> Vec<protocol::LspTextEdit> {
    match result {
        Value::Array(arr) => arr
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
        _ => vec![],
    }
}

fn parse_code_actions(result: Value) -> Vec<Value> {
    match result {
        Value::Array(arr) => arr
            .into_iter()
            .filter(|v| v.get("title").is_some())
            .collect(),
        _ => vec![],
    }
}

/// Test-only re-exports of private routing functions so unit tests can call them.
#[cfg(test)]
pub mod mod_fns {
    use super::*;

    pub fn route_response_pub(
        method: &str,
        uri: Option<&str>,
        result: Value,
    ) -> Option<LspMessage> {
        route_response(method, uri, result)
    }

    pub fn route_notification_pub(method: &str, params: Value) -> Option<LspMessage> {
        route_notification(method, params)
    }

    pub fn extract_hover_text_pub(result: &Value) -> Option<String> {
        extract_hover_text(result)
    }
}

#[cfg(test)]
mod annotation_tests;
#[cfg(test)]
mod tests;
// Re-export LspMessage for test access
pub use protocol::{LspDiagnostic, LspLocation, LspTextEdit};
