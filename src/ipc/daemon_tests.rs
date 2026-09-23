use super::*;
use crate::ipc::protocol::b64_decode;
use crate::transport::{read_framed, write_framed};
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, sync_channel};
use std::time::Duration;

fn connected_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let client = TcpStream::connect(addr).unwrap();
    let (server, _) = listener.accept().unwrap();
    (client, server)
}

fn send_initialize(stream: &mut TcpStream, token: &str, rows: u16, cols: u16) {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "token": token,
            "client_name": "test",
            "viewport": { "rows": rows, "cols": cols }
        }
    });
    write_framed(stream, &msg).unwrap();
}

fn do_handshake_client(client: &mut TcpStream, token: &str, rows: u16, cols: u16) {
    send_initialize(client, token, rows, cols);
    let _resp = read_framed(&mut BufReader::new(client as &TcpStream)).unwrap();
}

type Channels = (
    SyncSender<Key>,
    Receiver<Key>,
    std::sync::mpsc::Sender<Vec<u8>>,
    Receiver<Vec<u8>>,
    SyncSender<()>,
    Receiver<()>,
);

fn make_channels() -> Channels {
    let (input_tx, input_rx) = sync_channel::<Key>(64);
    let (output_tx, output_rx) = channel::<Vec<u8>>();
    let (detach_tx, detach_rx) = sync_channel::<()>(1);
    (
        input_tx, input_rx, output_tx, output_rx, detach_tx, detach_rx,
    )
}

// --- install_sigterm_handler ordering tests ---

// raise() runs the handler synchronously so it can't reproduce the race;
// this hooks the midpoint between publish and OS-level install instead.
#[cfg(unix)]
#[test]
fn flag_is_published_before_os_handler_is_installed() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut seen_published = false;
    install_sigterm_handler_with(Arc::clone(&shutdown), || {
        seen_published = !SHUTDOWN_FLAG.load(Ordering::Relaxed).is_null();
    });
    assert!(seen_published);
    sigterm_handler(15);
    assert!(shutdown.load(Ordering::Relaxed));
}

// --- forward_key tests ---

#[test]
fn forward_key_blocks_instead_of_dropping_under_burst() {
    let (input_tx, input_rx) = sync_channel::<Key>(64);
    const TOTAL: usize = 100;
    let sender = std::thread::spawn(move || {
        for i in 0..TOTAL {
            let c = char::from_u32('a' as u32 + (i % 26) as u32).unwrap();
            assert!(forward_key(&input_tx, Key::Char(c)));
        }
    });
    let mut received = 0;
    while received < TOTAL {
        if input_rx.recv_timeout(Duration::from_secs(2)).is_ok() {
            received += 1;
        } else {
            break;
        }
    }
    sender.join().unwrap();
    assert_eq!(received, TOTAL);
}

// --- parse_resize_params tests ---

#[test]
fn resize_params_wrapping_value_clamped_not_wrapped() {
    let params = serde_json::json!({"cols": 65536, "rows": 65616});
    let key = parse_resize_params(&params);
    assert_eq!(key, Key::Resize(80, 24));
}

#[test]
fn resize_params_zero_clamped_to_default() {
    let params = serde_json::json!({"cols": 0, "rows": 0});
    let key = parse_resize_params(&params);
    assert_eq!(key, Key::Resize(80, 24));
}

#[test]
fn resize_params_valid_value_passed_through() {
    let params = serde_json::json!({"cols": 120, "rows": 40});
    let key = parse_resize_params(&params);
    assert_eq!(key, Key::Resize(120, 40));
}

// --- do_handshake tests ---

#[test]
fn handshake_valid_token_returns_viewport_size() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let token = "mytoken";
    let handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(server);
        do_handshake(&mut reader, token)
    });
    send_initialize(&mut client, token, 40, 120);
    let _resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let size = handle.join().unwrap().unwrap();
    assert_eq!(size.rows, 40);
    assert_eq!(size.cols, 120);
}

#[test]
fn handshake_invalid_token_sends_unauthorized() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(server);
        do_handshake(&mut reader, "correct")
    });
    send_initialize(&mut client, "wrong", 24, 80);
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert_eq!(resp["error"]["code"], ERR_UNAUTHORIZED);
    assert!(handle.join().unwrap().is_err());
}

#[test]
fn handshake_wrong_method_returns_err() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(server);
        do_handshake(&mut reader, "tok")
    });
    let msg = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"ping","params":{}});
    write_framed(&mut client, &msg).unwrap();
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert!(resp["error"].is_object());
    assert!(handle.join().unwrap().is_err());
}

#[test]
fn handshake_response_has_session_id() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let token = "tok2";
    let handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(server);
        do_handshake(&mut reader, token)
    });
    send_initialize(&mut client, token, 24, 80);
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert!(resp["result"]["session_id"].as_str().is_some());
    handle.join().unwrap().unwrap();
}

#[test]
fn handshake_viewport_size_passed_through() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let token = "tok3";
    let handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(server);
        do_handshake(&mut reader, token)
    });
    send_initialize(&mut client, token, 80, 24);
    let _resp = read_framed(&mut BufReader::new(&client)).unwrap();
    let size = handle.join().unwrap().unwrap();
    assert_eq!(size.rows, 80);
    assert_eq!(size.cols, 24);
}

// --- serve_client tests ---

#[test]
fn serve_session_detach_returns_detach() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (input_tx, input_rx, _output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "detach_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    let msg = serde_json::json!({"jsonrpc":"2.0","method":"session.detach","params":{}});
    write_framed(&mut client, &msg).unwrap();
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert_eq!(resp["method"], "session.ending");
    assert_eq!(resp["params"]["reason"], "detached");
    drop(input_rx);
    drop(client);
    assert!(matches!(handle.join().unwrap(), ServeResult::Detach));
}

#[test]
fn serve_editor_quit_sends_session_ending() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let (input_tx, input_rx, output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "quit_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    drop(output_tx);
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert_eq!(resp["params"]["reason"], "editor-quit");
    drop(input_rx);
    drop(client);
    assert!(matches!(handle.join().unwrap(), ServeResult::EditorExited));
}

#[test]
fn serve_input_key_routes_to_editor() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (input_tx, input_rx, _output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "key_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    let msg =
        serde_json::json!({"jsonrpc":"2.0","method":"input.key","params":{"key":"a","seq":1}});
    write_framed(&mut client, &msg).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let mut got_char_a = false;
    while let Ok(k) = input_rx.try_recv() {
        if k == Key::Char('a') {
            got_char_a = true;
        }
    }
    assert!(got_char_a);
    write_framed(
        &mut client,
        &serde_json::json!({"jsonrpc":"2.0","method":"session.detach","params":{}}),
    )
    .unwrap();
    drop(client);
    handle.join().unwrap();
}

#[test]
fn serve_resize_routes_to_editor() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (input_tx, input_rx, _output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "resize_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    let msg =
        serde_json::json!({"jsonrpc":"2.0","method":"resize","params":{"cols":100,"rows":50}});
    write_framed(&mut client, &msg).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let mut got_resize = false;
    while let Ok(k) = input_rx.try_recv() {
        if k == Key::Resize(100, 50) {
            got_resize = true;
        }
    }
    assert!(got_resize);
    write_framed(
        &mut client,
        &serde_json::json!({"jsonrpc":"2.0","method":"session.detach","params":{}}),
    )
    .unwrap();
    drop(client);
    handle.join().unwrap();
}

#[test]
fn serve_render_update_sent_to_client() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (input_tx, input_rx, output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "render_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    // Let serve_client finish post-handshake drain before sending render data.
    std::thread::sleep(Duration::from_millis(50));
    output_tx.send(b"SCREEN_DATA".to_vec()).unwrap();
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert_eq!(resp["method"], "render.update");
    let decoded = b64_decode(resp["params"]["screen"].as_str().unwrap()).unwrap();
    assert_eq!(decoded, b"SCREEN_DATA");
    drop(output_tx);
    drop(input_rx);
    drop(client);
    handle.join().unwrap();
}

#[test]
fn serve_editor_detach_signal_returns_detach() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (input_tx, input_rx, _output_tx, output_rx, detach_tx, detach_rx) = make_channels();
    let token = "edetach_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    // Let serve_client complete post-handshake drain before signaling detach.
    std::thread::sleep(Duration::from_millis(50));
    detach_tx.send(()).unwrap();
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert_eq!(resp["params"]["reason"], "detached");
    drop(input_rx);
    drop(client);
    assert!(matches!(handle.join().unwrap(), ServeResult::Detach));
}

#[test]
fn serve_seq_echoed_in_render_update() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (input_tx, input_rx, output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "seq_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    std::thread::sleep(Duration::from_millis(50));
    let key_msg =
        serde_json::json!({"jsonrpc":"2.0","method":"input.key","params":{"key":"a","seq":42}});
    write_framed(&mut client, &key_msg).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    output_tx.send(b"frame".to_vec()).unwrap();
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert_eq!(resp["params"]["last_seq"], 42);
    drop(output_tx);
    drop(input_rx);
    drop(client);
    handle.join().unwrap();
}

#[test]
fn serve_stale_renders_drained_on_connect() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (input_tx, input_rx, output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    output_tx.send(b"stale".to_vec()).unwrap();
    let token = "stale_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    // Wait for post-handshake drain to complete before sending fresh render.
    std::thread::sleep(Duration::from_millis(50));
    output_tx.send(b"fresh".to_vec()).unwrap();
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    let decoded = b64_decode(resp["params"]["screen"].as_str().unwrap()).unwrap();
    assert_eq!(decoded, b"fresh");
    drop(output_tx);
    drop(input_rx);
    drop(client);
    handle.join().unwrap();
}

#[test]
fn serve_bad_token_returns_error() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (input_tx, input_rx, _output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            "correct",
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    send_initialize(&mut client, "wrong", 24, 80);
    drop(input_rx);
    assert!(matches!(handle.join().unwrap(), ServeResult::Error));
}

#[test]
fn serve_multiple_render_frames_ordered() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (input_tx, input_rx, output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "frames_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    std::thread::sleep(Duration::from_millis(50));
    for frame in [b"one" as &[u8], b"two", b"three"] {
        output_tx.send(frame.to_vec()).unwrap();
    }
    let mut buf_client = BufReader::new(&client);
    let mut frames = Vec::new();
    for _ in 0..3 {
        let bytes = read_framed(&mut buf_client).unwrap();
        let msg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let decoded = b64_decode(msg["params"]["screen"].as_str().unwrap()).unwrap();
        frames.push(decoded);
    }
    assert_eq!(frames[0], b"one");
    assert_eq!(frames[1], b"two");
    assert_eq!(frames[2], b"three");
    drop(buf_client);
    drop(output_tx);
    drop(input_rx);
    drop(client);
    handle.join().unwrap();
}

#[test]
fn serve_client_disconnect_returns_error() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (input_tx, input_rx, _output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "disc_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    drop(client);
    drop(input_rx);
    assert!(matches!(handle.join().unwrap(), ServeResult::Error));
}

#[test]
fn serve_unknown_method_ignored() {
    let (mut client, server) = connected_pair();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (input_tx, input_rx, _output_tx, output_rx, _detach_tx, detach_rx) = make_channels();
    let token = "unknown_tok";
    let handle = std::thread::spawn(move || {
        serve_client(
            BufReader::new(server),
            token,
            &input_tx,
            &output_rx,
            &detach_rx,
        )
    });
    do_handshake_client(&mut client, token, 24, 80);
    write_framed(
        &mut client,
        &serde_json::json!({"jsonrpc":"2.0","method":"no.such.method","params":{}}),
    )
    .unwrap();
    write_framed(
        &mut client,
        &serde_json::json!({"jsonrpc":"2.0","method":"session.detach","params":{}}),
    )
    .unwrap();
    let resp_bytes = read_framed(&mut BufReader::new(&client)).unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert_eq!(resp["method"], "session.ending");
    drop(input_rx);
    drop(client);
    handle.join().unwrap();
}

/// Joins `handle` on a watchdog thread so a hung reader thread fails the
/// test fast instead of blocking the whole suite. Returns None on timeout.
fn join_with_timeout<T: Send + 'static>(
    handle: std::thread::JoinHandle<T>,
    timeout: Duration,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(handle.join());
    });
    rx.recv_timeout(timeout).ok().map(|r| r.unwrap())
}

/// Reproduces the reader thread's loop as `serve_client` spawns it, so
/// shutdown behavior can be verified without a real cross-clone shutdown.
fn spawn_reader_loop(
    mut reader: BufReader<TcpStream>,
    shutdown: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    let (msg_tx, _msg_rx) = std::sync::mpsc::channel::<std::io::Result<Vec<u8>>>();
    std::thread::spawn(move || reader_loop(&mut reader, &shutdown, &msg_tx))
}

/// Without a read timeout, setting only the shutdown flag (no data, no
/// socket-level shutdown) would leave the reader blocked forever.
#[test]
fn reader_thread_exits_on_shutdown_flag_without_socket_shutdown() {
    let (client, server) = connected_pair();
    let shutdown = Arc::new(AtomicBool::new(false));
    let reader_thread = spawn_reader_loop(BufReader::new(server), Arc::clone(&shutdown));

    // Client sends nothing further and the socket itself is never shut
    // down, simulating the cross-clone signal failing to propagate.
    std::thread::sleep(Duration::from_millis(150));
    shutdown.store(true, Ordering::Relaxed);

    let joined = join_with_timeout(reader_thread, Duration::from_secs(3));
    assert!(joined.is_some(), "reader thread did not exit in time");
    drop(client);
}
