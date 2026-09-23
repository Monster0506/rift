use super::*;

impl LspClient {
    /// Build a client around an arbitrary writer with no real process/reader
    /// behind it, for unit-testing the send path in isolation.
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

    /// OS process id of the server, for tests verifying it doesn't leak.
    pub(crate) fn pid(&self) -> u32 {
        self._process.as_ref().expect("stdio client").id()
    }
}

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
    let parsed = parse_rpc_message(msg(Some(Value::from(7)), Some("client/registerCapability")));
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

    let typed_bytes = typed_sink.lock().clone();
    let value_bytes = value_sink.lock().clone();

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

    let written = sink.lock().clone();
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
    sink: std::sync::Arc<parking_lot::Mutex<Vec<u8>>>,
}

impl Write for SlowWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        std::thread::sleep(self.delay);
        self.sink.lock().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn client_with_slow_writer(
    delay: std::time::Duration,
) -> (LspClient, std::sync::Arc<parking_lot::Mutex<Vec<u8>>>) {
    let sink = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
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

    let written = sink.lock().clone();
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
