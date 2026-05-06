use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
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
        id: u64,
        method: String,
        params: Value,
    },
    /// A JSON parse error on a message from the server.
    ParseError { message: String },
}

/// One live connection to a language server process.
pub struct LspClient {
    pub language: String,
    _process: Child,
    stdin: ChildStdin,
    next_id: u64,
    /// Maps request id → method name so responses can be routed.
    pub pending: HashMap<u64, String>,
    receiver: Receiver<RawLspMessage>,
    _reader_thread: thread::JoinHandle<()>,
    pub initialized: bool,
    pub root_uri: Option<String>,
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

        Ok(Self {
            language,
            _process: process,
            stdin,
            next_id: 1,
            pending: HashMap::new(),
            receiver: rx,
            _reader_thread: reader_thread,
            initialized: false,
            root_uri,
        })
    }

    /// Send a JSON-RPC request and return its id.
    pub fn send_request(&mut self, method: impl Into<String>, params: Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let method = method.into();
        self.pending.insert(id, method.clone());
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
    pub fn send_response(&mut self, id: u64, result: Value) {
        #[derive(serde::Serialize)]
        struct Response {
            jsonrpc: &'static str,
            id: u64,
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

    fn write_message<T: serde::Serialize>(&mut self, msg: &T) {
        if let Ok(body) = serde_json::to_vec(msg) {
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            let _ = self.stdin.write_all(header.as_bytes());
            let _ = self.stdin.write_all(&body);
            let _ = self.stdin.flush();
        }
    }

    /// Drain all pending raw messages from the reader thread.
    pub fn poll_raw(&mut self) -> Vec<RawLspMessage> {
        let mut msgs = Vec::new();
        while let Ok(m) = self.receiver.try_recv() {
            msgs.push(m);
        }
        msgs
    }
}

fn spawn_reader_thread(stdout: ChildStdout, tx: Sender<RawLspMessage>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            // Read headers
            let mut content_length: Option<usize> = None;
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => {
                        return;
                    }
                    Ok(_) => {}
                }
                let line = line.trim();
                if line.is_empty() {
                    break; // End of headers
                }
                if let Some(rest) = line.strip_prefix("Content-Length:") {
                    if let Ok(n) = rest.trim().parse::<usize>() {
                        content_length = Some(n);
                    }
                }
            }

            let n = match content_length {
                Some(n) => n,
                None => {
                    continue;
                }
            };

            // Read body
            let mut body = vec![0u8; n];
            if read_exact(&mut reader, &mut body).is_err() {
                return;
            }

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

fn read_exact<R: std::io::Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<()> {
    let mut offset = 0;
    while offset < buf.len() {
        match reader.read(&mut buf[offset..]) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "EOF",
                ))
            }
            Ok(n) => offset += n,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn parse_rpc_message(msg: JsonRpcMessage) -> Option<RawLspMessage> {
    if let Some(method) = msg.method {
        let params = msg.params.unwrap_or(Value::Null);
        // Server-initiated request has both method and id; needs a response.
        if let Some(Value::Number(n)) = &msg.id {
            if let Some(id) = n.as_u64() {
                return Some(RawLspMessage::ServerRequest { id, method, params });
            }
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

    if let Some(result) = msg.result {
        return Some(RawLspMessage::Response { id, result });
    }

    None
}
