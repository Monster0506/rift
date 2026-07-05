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

/// One live connection to a language server, either a spawned child process
/// (stdio) or a keepalive broker socket that outlives this editor session.
pub struct LspClient {
    pub language: String,
    _process: Option<Child>,
    writer: Box<dyn Write + Send>,
    next_id: u64,
    /// Maps request id -> method name so responses can be routed.
    pub pending: HashMap<u64, String>,
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

        Ok(Self {
            language,
            _process: Some(process),
            writer: Box::new(stdin),
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

        Ok(Self {
            language,
            _process: None,
            writer: Box::new(writer),
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

    fn write_message<T: serde::Serialize>(&mut self, msg: &T) {
        let _ = crate::transport::write_framed(&mut self.writer, msg);
    }

    /// OS process id of the server, for tests verifying it doesn't leak.
    #[cfg(test)]
    pub(crate) fn pid(&self) -> u32 {
        self._process.as_ref().expect("stdio client").id()
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

    if let Some(result) = msg.result {
        return Some(RawLspMessage::Response { id, result });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: Option<Value>, method: Option<&str>) -> JsonRpcMessage {
        JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.map(str::to_string),
            params: Some(Value::Null),
            result: None,
            error: None,
        }
    }

    #[test]
    fn server_request_with_string_id_is_not_misrouted_as_notification() {
        let parsed = parse_rpc_message(msg(
            Some(Value::String("req-1".to_string())),
            Some("workspace/configuration"),
        ));
        match parsed {
            Some(RawLspMessage::ServerRequest { id, method, .. }) => {
                assert_eq!(id, protocol::RequestId::String("req-1".to_string()));
                assert_eq!(method, "workspace/configuration");
            }
            other => panic!("expected ServerRequest, got {other:?}"),
        }
    }

    #[test]
    fn server_request_with_numeric_id_still_works() {
        let parsed =
            parse_rpc_message(msg(Some(Value::from(7)), Some("client/registerCapability")));
        match parsed {
            Some(RawLspMessage::ServerRequest { id, .. }) => {
                assert_eq!(id, protocol::RequestId::Number(7));
            }
            other => panic!("expected ServerRequest, got {other:?}"),
        }
    }

    #[test]
    fn method_with_no_id_is_a_notification() {
        let parsed = parse_rpc_message(msg(None, Some("textDocument/publishDiagnostics")));
        assert!(matches!(parsed, Some(RawLspMessage::Notification { .. })));
    }

    fn process_is_running(pid: u32) -> bool {
        #[cfg(windows)]
        {
            let out = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}")])
                .output()
                .expect("tasklist");
            String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
    }

    #[test]
    fn drop_kills_and_reaps_the_child_process() {
        // A long-running, stdio-redirect-safe placeholder "server".
        let client = LspClient::start(
            "test".to_string(),
            "ping",
            &["-n".to_string(), "30".to_string(), "127.0.0.1".to_string()],
            None,
        )
        .expect("spawn ping");
        let pid = client.pid();
        assert!(process_is_running(pid), "ping should have started");

        drop(client);
        std::thread::sleep(std::time::Duration::from_millis(300));

        assert!(
            !process_is_running(pid),
            "Drop must kill+reap the child instead of leaking it"
        );
    }
}
