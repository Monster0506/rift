use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdout, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use serde_json::Value;

use super::protocol::{self, JsonRpcMessage};

/// A raw message coming back from the language server subprocess reader thread.
#[derive(Debug)]
pub enum RawLspMessage {
    /// A response to a request we sent (has id).
    Response { id: u64, result: Value },
    /// A response error.
    ResponseError { id: u64, message: String },
    /// A notification pushed by the server (no id).
    Notification { method: String, params: Value },
    /// A request from the server that needs a response (has both method and id).
    ServerRequest {
        id: protocol::RequestId,
        method: String,
        params: Value,
    },
    /// A JSON parse error on a message from the server.
    ParseError { message: String },
}

/// Metadata for an in-flight request, keyed by this client's request id.
#[derive(Debug)]
pub struct PendingRequest {
    pub method: String,
    /// Normalized URI of the document the request was issued for, if any.
    pub uri: Option<String>,
}

/// One live connection to a language server, either a spawned child process
/// (stdio) or a keepalive broker socket that outlives this editor session.
pub struct LspClient {
    pub language: String,
    _process: Option<Child>,
    /// Pre-framed message bytes queued for the writer thread; the blocking
    /// I/O to a possibly-slow peer happens off this thread entirely.
    writer_tx: Sender<Vec<u8>>,
    _writer_thread: thread::JoinHandle<()>,
    next_id: u64,
    /// Request ids are per client, so routing must consult this map rather
    /// than a shared one (two servers both start numbering at 1).
    pub pending: HashMap<u64, PendingRequest>,
    receiver: Receiver<RawLspMessage>,
    _reader_thread: thread::JoinHandle<()>,
    pub initialized: bool,
    pub root_uri: Option<String>,
}

impl Drop for LspClient {
    /// Kill and reap an owned server process so it (and the reader thread
    /// blocked on its stdout) don't leak. Broker connections just close.
    fn drop(&mut self) {
        if let Some(process) = &mut self._process {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

impl LspClient {
    /// Spawn the language server process and set up I/O threads.
    pub fn start(
        language: String,
        command: &str,
        args: &[String],
        root_uri: Option<String>,
    ) -> anyhow::Result<Self> {
        let mut process = std::process::Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = process.stdin.take().expect("stdin");
        let stdout: ChildStdout = process.stdout.take().expect("stdout");

        let (tx, rx) = mpsc::channel::<RawLspMessage>();
        let reader_thread = spawn_reader_thread(stdout, tx);
        let (writer_tx, writer_rx) = mpsc::channel::<Vec<u8>>();
        let writer_thread = spawn_writer_thread(Box::new(stdin), writer_rx);

        Ok(Self {
            language,
            _process: Some(process),
            writer_tx,
            _writer_thread: writer_thread,
            next_id: 1,
            pending: HashMap::new(),
            receiver: rx,
            _reader_thread: reader_thread,
            initialized: false,
            root_uri,
        })
    }

    /// Attach to (or spawn) the keepalive broker for this server, so the
    /// server and its warm index survive across editor sessions.
    pub fn start_keepalive(
        language: String,
        command: &str,
        args: &[String],
        root_uri: Option<String>,
    ) -> anyhow::Result<Self> {
        let (reader, writer) = super::broker::attach(command, args, root_uri.as_deref())?;

        let (tx, rx) = mpsc::channel::<RawLspMessage>();
        let reader_thread = spawn_reader_thread(reader, tx);
        let (writer_tx, writer_rx) = mpsc::channel::<Vec<u8>>();
        let writer_thread = spawn_writer_thread(Box::new(writer), writer_rx);

        Ok(Self {
            language,
            _process: None,
            writer_tx,
            _writer_thread: writer_thread,
            next_id: 1,
            pending: HashMap::new(),
            receiver: rx,
            _reader_thread: reader_thread,
            initialized: false,
            root_uri,
        })
    }

    /// Send a JSON-RPC request and return its id. `uri` is remembered so the
    /// response can be attributed to the document it was issued for.
    pub fn send_request(
        &mut self,
        method: impl Into<String>,
        params: Value,
        uri: Option<String>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let method = method.into();
        self.pending.insert(
            id,
            PendingRequest {
                method: method.clone(),
                uri,
            },
        );
        let req = protocol::JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params: Some(params),
        };
        self.write_message(&req);
        id
    }

    /// Send a JSON-RPC response to a server-initiated request.
    pub fn send_response(&mut self, id: protocol::RequestId, result: Value) {
        #[derive(serde::Serialize)]
        struct Response {
            jsonrpc: &'static str,
            id: protocol::RequestId,
            result: Value,
        }
        self.write_message(&Response {
            jsonrpc: "2.0",
            id,
            result,
        });
    }

    /// Send a JSON-RPC notification (no id, no response expected).
    pub fn send_notification(&mut self, method: impl Into<String>, params: Value) {
        let notif = protocol::JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.into(),
            params: Some(params),
        };
        self.write_message(&notif);
    }

    /// Like `send_notification`, but serializes `params` straight into the
    /// outer envelope, skipping the intermediate `serde_json::Value`.
    pub fn send_notification_typed<P: serde::Serialize>(
        &mut self,
        method: impl Into<String>,
        params: &P,
    ) {
        #[derive(serde::Serialize)]
        struct TypedNotification<'a, P> {
            jsonrpc: &'static str,
            method: String,
            params: &'a P,
        }
        self.write_message(&TypedNotification {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        });
    }

    fn write_message<T: serde::Serialize>(&mut self, msg: &T) {
        if let Ok(framed) = crate::transport::frame_message(msg) {
            let _ = self.writer_tx.send(framed);
        }
    }

    /// Drain all pending raw messages from the reader thread. The flag is
    /// true once the reader has hung up (server exited or pipe closed).
    pub fn poll_raw(&mut self) -> (Vec<RawLspMessage>, bool) {
        let mut msgs = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(m) => msgs.push(m),
                Err(mpsc::TryRecvError::Empty) => return (msgs, false),
                Err(mpsc::TryRecvError::Disconnected) => return (msgs, true),
            }
        }
    }
}

/// Owns the actual writer and drains queued frames onto it in order, so a
/// slow/stalled peer only ever blocks this thread, never the caller.
fn spawn_writer_thread(
    mut writer: Box<dyn Write + Send>,
    rx: Receiver<Vec<u8>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(framed) = rx.recv() {
            if writer.write_all(&framed).is_err() {
                return;
            }
            if writer.flush().is_err() {
                return;
            }
        }
    })
}

fn spawn_reader_thread<R: Read + Send + 'static>(
    source: R,
    tx: Sender<RawLspMessage>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(source);
        loop {
            let body = match crate::transport::read_framed(&mut reader) {
                Ok(b) => b,
                Err(_) => return,
            };

            let msg: JsonRpcMessage = match serde_json::from_slice(&body) {
                Ok(m) => m,
                Err(e) => {
                    let snippet = String::from_utf8_lossy(&body[..body.len().min(200)]);
                    let _ = tx.send(RawLspMessage::ParseError {
                        message: format!("JSON parse error: {} | body: {}", e, snippet),
                    });
                    continue;
                }
            };

            if let Some(raw) = parse_rpc_message(msg) {
                if tx.send(raw).is_err() {
                    return;
                }
            }
        }
    })
}

fn parse_rpc_message(msg: JsonRpcMessage) -> Option<RawLspMessage> {
    if let Some(method) = msg.method {
        let params = msg.params.unwrap_or(Value::Null);
        // A method with an id (number or string, both valid) is a
        // server-initiated request; with no id at all, it's a notification.
        let request_id = match &msg.id {
            Some(Value::Number(n)) => n.as_u64().map(protocol::RequestId::Number),
            Some(Value::String(s)) => Some(protocol::RequestId::String(s.clone())),
            _ => None,
        };
        if let Some(id) = request_id {
            return Some(RawLspMessage::ServerRequest { id, method, params });
        }
        return Some(RawLspMessage::Notification { method, params });
    }

    let id = match &msg.id {
        Some(Value::Number(n)) => n.as_u64()?,
        _ => return None,
    };

    if let Some(error) = msg.error {
        return Some(RawLspMessage::ResponseError {
            id,
            message: error.message,
        });
    }

    // `"result": null` deserializes to None; it is still a valid response
    // (e.g. definition/hover with nothing found) and must be routed.
    Some(RawLspMessage::Response {
        id,
        result: msg.result.unwrap_or(Value::Null),
    })
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod client_tests;
