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

    /// Build a client around an arbitrary writer with no real process/reader
    /// behind it, for unit-testing the send path in isolation.
    #[cfg(test)]
    fn new_for_test(writer: Box<dyn Write + Send>) -> Self {
        let (tx, rx) = mpsc::channel::<RawLspMessage>();
        let reader_thread = spawn_reader_thread(std::io::empty(), tx);
        let (writer_tx, writer_rx) = mpsc::channel::<Vec<u8>>();
        let writer_thread = spawn_writer_thread(writer, writer_rx);

        Self {
            language: "test".to_string(),
            _process: None,
            writer_tx,
            _writer_thread: writer_thread,
            next_id: 1,
            pending: HashMap::new(),
            receiver: rx,
            _reader_thread: reader_thread,
            initialized: false,
            root_uri: None,
        }
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

    /// OS process id of the server, for tests verifying it doesn't leak.
    #[cfg(test)]
    pub(crate) fn pid(&self) -> u32 {
        self._process.as_ref().expect("stdio client").id()
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

    #[test]
    fn response_with_null_result_is_still_a_response() {
        // `{"id":3,"result":null}` is how servers say "nothing found"; it
        // used to be dropped on the floor, leaking the pending request.
        let parsed: JsonRpcMessage =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":3,"result":null}"#).unwrap();
        match parse_rpc_message(parsed) {
            Some(RawLspMessage::Response { id, result }) => {
                assert_eq!(id, 3);
                assert!(result.is_null());
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct TestParams {
        uri: String,
        version: i64,
    }

    #[test]
    fn send_notification_typed_produces_the_same_wire_format_as_send_notification() {
        let params = TestParams {
            uri: "file:///a.rs".to_string(),
            version: 3,
        };

        let (mut typed_client, typed_sink) = client_with_slow_writer(std::time::Duration::ZERO);
        typed_client.send_notification_typed("textDocument/didChange", &params);

        let (mut value_client, value_sink) = client_with_slow_writer(std::time::Duration::ZERO);
        let value = serde_json::to_value(&params).unwrap();
        value_client.send_notification("textDocument/didChange", value);

        std::thread::sleep(std::time::Duration::from_millis(100));

        let typed_bytes = typed_sink.lock().unwrap().clone();
        let value_bytes = value_sink.lock().unwrap().clone();

        // Same framed bytes: the typed path isn't just equivalent, it's
        // byte-identical to what the Value round-trip would have sent.
        assert_eq!(typed_bytes, value_bytes);
    }

    #[test]
    fn send_notification_typed_round_trips_through_the_wire_format() {
        let params = TestParams {
            uri: "file:///b.rs".to_string(),
            version: 7,
        };
        let (mut client, sink) = client_with_slow_writer(std::time::Duration::ZERO);
        client.send_notification_typed("textDocument/didChange", &params);
        std::thread::sleep(std::time::Duration::from_millis(100));

        let written = sink.lock().unwrap().clone();
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(written));
        let body = crate::transport::read_framed(&mut reader).unwrap();
        let parsed: JsonRpcMessage = serde_json::from_slice(&body).unwrap();

        assert_eq!(parsed.method.as_deref(), Some("textDocument/didChange"));
        let round_tripped: TestParams = serde_json::from_value(parsed.params.unwrap()).unwrap();
        assert_eq!(round_tripped, params);
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

    /// A `Write` that sleeps before recording each write, so a test can
    /// prove a caller returned before the slow I/O finished.
    struct SlowWriter {
        delay: std::time::Duration,
        sink: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl Write for SlowWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            std::thread::sleep(self.delay);
            self.sink.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn client_with_slow_writer(
        delay: std::time::Duration,
    ) -> (LspClient, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
        let sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let client = LspClient::new_for_test(Box::new(SlowWriter {
            delay,
            sink: sink.clone(),
        }));
        (client, sink)
    }

    #[test]
    fn send_notification_returns_before_a_slow_write_completes() {
        let (mut client, _sink) = client_with_slow_writer(std::time::Duration::from_millis(300));

        let start = std::time::Instant::now();
        client.send_notification("test/method", serde_json::json!({"x": 1}));
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "send_notification should enqueue and return immediately, took {elapsed:?}"
        );
    }

    #[test]
    fn send_request_also_does_not_block_on_a_slow_write() {
        let (mut client, _sink) = client_with_slow_writer(std::time::Duration::from_millis(300));

        let start = std::time::Instant::now();
        let id = client.send_request("test/request", serde_json::json!({}), None);
        let elapsed = start.elapsed();

        assert_eq!(id, 1);
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "send_request should enqueue and return immediately, took {elapsed:?}"
        );
    }

    #[test]
    fn queued_writes_eventually_land_in_order() {
        let (mut client, sink) = client_with_slow_writer(std::time::Duration::from_millis(20));

        for i in 0..5 {
            client.send_notification(format!("test/method{i}"), serde_json::json!({}));
        }

        // Give the background writer thread time to drain all 5 (~20ms each).
        std::thread::sleep(std::time::Duration::from_millis(500));

        let written = sink.lock().unwrap().clone();
        let text = String::from_utf8_lossy(&written);
        let positions: Vec<usize> = (0..5)
            .map(|i| {
                text.find(&format!("test/method{i}"))
                    .unwrap_or_else(|| panic!("method{i} never written"))
            })
            .collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "writes must land in enqueue order, got offsets {positions:?}"
        );
    }

    #[test]
    fn many_queued_writes_never_block_the_caller() {
        // Every write sleeps 50ms; enqueueing 50 of them must still stay fast.
        let (mut client, _sink) = client_with_slow_writer(std::time::Duration::from_millis(50));

        let start = std::time::Instant::now();
        for i in 0..50 {
            client.send_notification(format!("test/burst{i}"), serde_json::json!({"i": i}));
        }
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "50 enqueues should not wait on any actual I/O, took {elapsed:?}"
        );
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
