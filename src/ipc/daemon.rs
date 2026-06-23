use std::io::{BufReader, ErrorKind};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use crate::ipc::channel_backend::ChannelBackend;
use crate::ipc::protocol::{
    b64_encode, InitializeParams, RenderUpdateParams, SessionEndingParams, ERR_UNAUTHORIZED,
};
use crate::ipc::session::{data_dir, generate_token, session_path, SessionInfo};
use crate::key::Key;
use crate::term::Size;
use crate::transport::{read_framed, write_framed};

enum ServeResult {
    /// Client sent session.detach -- editor still running, wait for next connection.
    Detach,
    /// Editor exited (`:q`) -- client notified; daemon should restart the editor.
    EditorExited,
    /// TCP error -- client disconnected unexpectedly, editor still running.
    Error,
}

struct EditorInstance {
    thread: std::thread::JoinHandle<()>,
    input_tx: SyncSender<Key>,
    output_rx: Receiver<Vec<u8>>,
    detach_rx: Receiver<()>,
}

fn spawn_editor(file: Option<String>) -> EditorInstance {
    let (input_tx, input_rx) = std::sync::mpsc::sync_channel::<Key>(64);
    let (output_tx, output_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let (detach_tx, detach_rx) = std::sync::mpsc::sync_channel::<()>(1);

    let thread = std::thread::spawn(move || {
        let backend =
            ChannelBackend::new(input_rx, output_tx, detach_tx, Size { rows: 24, cols: 80 });
        eprintln!("rift: editor thread starting");
        match crate::editor::Editor::with_file(backend, file) {
            Ok(mut e) => {
                e.set_remote(true);
                eprintln!("rift: editor initialized, running");
                let r = e.run();
                eprintln!(
                    "rift: editor run returned: {:?}",
                    r.as_ref().map(|_| "ok").unwrap_or("err")
                );
            }
            Err(e) => eprintln!("rift: editor init error: {e}"),
        }
        eprintln!("rift: editor thread exiting");
    });

    EditorInstance {
        thread,
        input_tx,
        output_rx,
        detach_rx,
    }
}

pub fn run(file: Option<String>) -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();

    let token = generate_token();
    let pid = std::process::id();
    let sess_path = session_path(pid);
    let info = SessionInfo {
        pid,
        host: "127.0.0.1".into(),
        port,
        token: token.clone(),
    };
    crate::ipc::session::write(&info, &sess_path)?;
    let _guard = SessionGuard(sess_path.clone());

    eprintln!("rift daemon  pid={pid}  port={port}  token={token}");
    eprintln!("session: {}", sess_path.display());

    let shutdown = Arc::new(AtomicBool::new(false));
    install_sigterm_handler(Arc::clone(&shutdown));

    let mut instance = spawn_editor(file.clone());

    loop {
        if shutdown.load(Ordering::Relaxed) {
            eprintln!("rift: SIGTERM received, shutting down");
            break;
        }

        listener.set_nonblocking(true)?;
        let pair = loop {
            if shutdown.load(Ordering::Relaxed) {
                break None;
            }
            match listener.accept() {
                Ok(p) => break Some(p),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(e) => {
                    eprintln!("rift accept error: {e}");
                    break None;
                }
            }
        };
        listener.set_nonblocking(false)?;

        let (stream, peer) = match pair {
            Some(p) => p,
            None => break,
        };

        // The accepted socket inherits non-blocking mode from the listener on
        // Windows; restore blocking so timeouts in serve_client work correctly.
        if let Err(e) = stream.set_nonblocking(false) {
            eprintln!("rift: failed to set stream blocking: {e}");
            continue;
        }

        eprintln!("rift: client connected from {peer}");

        if instance.thread.is_finished() {
            eprintln!("rift: editor was not running, starting fresh");
            instance = spawn_editor(file.clone());
        }

        let reader = BufReader::new(stream);
        match serve_client(
            reader,
            &token,
            &instance.input_tx,
            &instance.output_rx,
            &instance.detach_rx,
        ) {
            ServeResult::Detach => {
                eprintln!("rift: client detached, waiting for reconnect...");
            }
            ServeResult::EditorExited => {
                eprintln!("rift: editor quit, restarting for next session...");
                let _ = instance.thread.join();
                instance = spawn_editor(file.clone());
            }
            ServeResult::Error => {
                eprintln!("rift: client disconnected unexpectedly, waiting for reconnect...");
            }
        }
    }

    let _ = instance.thread.join();
    Ok(())
}

/// Re-launch the current process without `--detach`/`-d`, running in the background.
/// Polls until the daemon writes its session file, then prints startup info.
pub fn detach() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--detach" && a != "-d")
        .collect();

    let sessions_dir = data_dir().join("sessions");
    let existing: std::collections::HashSet<std::path::PathBuf> = std::fs::read_dir(&sessions_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        use std::process::Command;
        extern "C" {
            fn setsid() -> i32;
        }
        // Safety: setsid is async-signal-safe; calling it in the child after fork
        // creates a new session, detaching from the parent's controlling terminal
        // so the daemon survives SIGHUP when the SSH session that started it closes.
        unsafe {
            Command::new(&exe)
                .args(&args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .pre_exec(|| {
                    setsid();
                    Ok(())
                })
                .spawn()?;
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        // CREATE_NO_WINDOW hides the console window while keeping a valid
        // console context so Win32 APIs (arboard, etc.) work correctly.
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        Command::new(&exe)
            .args(&args)
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
            .spawn()?;
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let found = loop {
        if std::time::Instant::now() > deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
        let new_file = std::fs::read_dir(&sessions_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .find(|p| !existing.contains(p) && p.extension().map(|x| x == "json").unwrap_or(false));
        if let Some(path) = new_file {
            if let Ok(info) = crate::ipc::session::read(&path) {
                break Some((info, path));
            }
        }
    };

    if let Some((info, sess_path)) = found {
        eprintln!(
            "rift daemon  pid={}  port={}  token={}",
            info.pid, info.port, info.token
        );
        eprintln!("session: {}", sess_path.display());
    }
    eprintln!("rift daemon started in background");
    Ok(())
}

#[cfg(unix)]
fn install_sigterm_handler(shutdown: Arc<AtomicBool>) {
    extern "C" {
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> extern "C" fn(i32);
    }
    let _ = unsafe { signal(15, sigterm_handler) };
    // Safety: only written once before any signal can fire.
    SHUTDOWN_FLAG.store(shutdown.as_ptr() as *mut _, Ordering::Relaxed);
    std::mem::forget(shutdown);
}

#[cfg(unix)]
static SHUTDOWN_FLAG: std::sync::atomic::AtomicPtr<AtomicBool> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

#[cfg(unix)]
extern "C" fn sigterm_handler(_: i32) {
    let ptr = SHUTDOWN_FLAG.load(Ordering::Relaxed);
    if !ptr.is_null() {
        unsafe { (*ptr).store(true, Ordering::Relaxed) };
    }
}

#[cfg(not(unix))]
fn install_sigterm_handler(_shutdown: Arc<AtomicBool>) {}

struct SessionGuard(std::path::PathBuf);

impl Drop for SessionGuard {
    fn drop(&mut self) {
        crate::ipc::session::remove(&self.0);
    }
}

fn send_error(
    stream: &mut impl std::io::Write,
    id: &serde_json::Value,
    code: i64,
    message: &str,
) -> anyhow::Result<()> {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    });
    write_framed(stream, &msg)?;
    Ok(())
}

fn do_handshake(reader: &mut BufReader<TcpStream>, expected_token: &str) -> anyhow::Result<Size> {
    let body = read_framed(reader)?;
    let msg: serde_json::Value = serde_json::from_slice(&body)?;
    let id = msg["id"].clone();
    let method = msg["method"].as_str().unwrap_or("").to_string();

    if method != "initialize" {
        let _ = send_error(
            reader.get_mut(),
            &id,
            ERR_UNAUTHORIZED,
            "expected initialize",
        );
        anyhow::bail!("unexpected method: {method}");
    }

    let params: InitializeParams = serde_json::from_value(msg["params"].clone())?;
    if params.token != expected_token {
        let _ = send_error(reader.get_mut(), &id, ERR_UNAUTHORIZED, "invalid token");
        anyhow::bail!("token mismatch");
    }

    let session_id = generate_token();
    let result = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "session_id": session_id },
    });
    write_framed(reader.get_mut(), &result)?;

    Ok(Size {
        rows: params.viewport.rows,
        cols: params.viewport.cols,
    })
}

/// How often the reader checks `shutdown` between blocking reads. Bounds how
/// long `join()` can take after a shutdown is requested.
const READER_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Reads frames from `reader`, sending each to `msg_tx`, until a real
/// disconnect or `shutdown` is set. A read timeout on the socket ensures the
/// loop notices `shutdown` even if no cross-clone socket shutdown arrives.
fn reader_loop(
    reader: &mut BufReader<TcpStream>,
    shutdown: &Arc<AtomicBool>,
    msg_tx: &std::sync::mpsc::Sender<std::io::Result<Vec<u8>>>,
) {
    reader
        .get_ref()
        .set_read_timeout(Some(READER_POLL_INTERVAL))
        .ok();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match read_framed(reader) {
            Ok(bytes) => {
                if msg_tx.send(Ok(bytes)).is_err() {
                    break;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                continue;
            }
            Err(e) => {
                let _ = msg_tx.send(Err(e));
                break;
            }
        }
    }
}

fn serve_client(
    mut reader: BufReader<TcpStream>,
    expected_token: &str,
    input_tx: &SyncSender<Key>,
    output_rx: &Receiver<Vec<u8>>,
    detach_rx: &Receiver<()>,
) -> ServeResult {
    // Timeout on handshake so a stale or malicious client can't block the daemon.
    reader
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok();
    let size = match do_handshake(&mut reader, expected_token) {
        Ok(s) => {
            eprintln!("rift: handshake ok, viewport {}x{}", s.cols, s.rows);
            s
        }
        Err(e) => {
            eprintln!("rift handshake error: {e}");
            return ServeResult::Error;
        }
    };
    reader.get_ref().set_read_timeout(None).ok();

    let mut write_stream = match reader.get_ref().try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("rift: stream clone failed: {e}");
            return ServeResult::Error;
        }
    };

    // Drain stale renders and any leftover detach signal, then trigger a fresh draw.
    while output_rx.try_recv().is_ok() {}
    while detach_rx.try_recv().is_ok() {}
    let _ = input_tx.try_send(Key::Resize(size.cols, size.rows));

    // Reader runs on its own thread with a periodic read timeout, so it can
    // notice `reader_shutdown` even if the cross-clone socket shutdown below
    // does not interrupt its blocking read on this platform.
    let reader_shutdown = Arc::new(AtomicBool::new(false));
    let reader_shutdown_clone = Arc::clone(&reader_shutdown);
    let (msg_tx, msg_rx) = std::sync::mpsc::channel::<std::io::Result<Vec<u8>>>();
    let reader_thread = std::thread::spawn(move || {
        reader_loop(&mut reader, &reader_shutdown_clone, &msg_tx);
    });

    let one_ms = Duration::from_millis(1);
    let mut last_seq: u64 = 0;
    let result;

    'serve: loop {
        match msg_rx.recv_timeout(one_ms) {
            Ok(Ok(bytes)) => {
                if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    match msg["method"].as_str().unwrap_or("") {
                        "session.detach" => {
                            let end = serde_json::json!({
                                "jsonrpc": "2.0",
                                "method": "session.ending",
                                "params": SessionEndingParams { reason: "detached".into() },
                            });
                            let _ = write_framed(&mut write_stream, &end);
                            result = ServeResult::Detach;
                            break 'serve;
                        }
                        "input.key" => {
                            if let Some(seq) = msg["params"]["seq"].as_u64() {
                                last_seq = seq;
                            }
                            if let Some(key_str) = msg["params"]["key"].as_str() {
                                if let Some(key) = crate::ipc::key_notation::key_to_vim(key_str) {
                                    let _ = input_tx.try_send(key);
                                }
                            }
                        }
                        "resize" => {
                            let cols = msg["params"]["cols"].as_u64().unwrap_or(80) as u16;
                            let rows = msg["params"]["rows"].as_u64().unwrap_or(24) as u16;
                            let _ = input_tx.try_send(Key::Resize(cols, rows));
                        }
                        _ => {}
                    }
                }
            }
            Ok(Err(_)) => {
                result = ServeResult::Error;
                break 'serve;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                result = ServeResult::Error;
                break 'serve;
            }
        }

        // Editor pressed :q in remote mode — detach client, keep editor alive.
        if detach_rx.try_recv().is_ok() {
            let end = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "session.ending",
                "params": SessionEndingParams { reason: "detached".into() },
            });
            let _ = write_framed(&mut write_stream, &end);
            result = ServeResult::Detach;
            break 'serve;
        }

        loop {
            match output_rx.try_recv() {
                Ok(render_bytes) => {
                    let params = RenderUpdateParams {
                        screen: b64_encode(&render_bytes),
                        last_seq,
                    };
                    let msg = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "render.update",
                        "params": params,
                    });
                    if write_framed(&mut write_stream, &msg).is_err() {
                        result = ServeResult::Error;
                        break 'serve;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let end = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "session.ending",
                        "params": SessionEndingParams { reason: "editor-quit".into() },
                    });
                    let _ = write_framed(&mut write_stream, &end);
                    result = ServeResult::EditorExited;
                    break 'serve;
                }
            }
        }
    }

    reader_shutdown.store(true, Ordering::Relaxed);
    let _ = write_stream.shutdown(std::net::Shutdown::Both);
    reader_thread.join().ok();
    result
}

#[cfg(test)]
mod tests {
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

    /// Reproduces the reader thread's blocking-read loop exactly as
    /// `serve_client` spawns it, so the shutdown behavior can be verified
    /// in isolation without depending on cross-clone socket shutdown.
    fn spawn_reader_loop(
        mut reader: BufReader<TcpStream>,
        shutdown: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        let (msg_tx, _msg_rx) = std::sync::mpsc::channel::<std::io::Result<Vec<u8>>>();
        std::thread::spawn(move || reader_loop(&mut reader, &shutdown, &msg_tx))
    }

    /// The cross-clone `.shutdown()` signal does not reliably interrupt a
    /// blocking `read_framed` call on every platform. Without a read timeout,
    /// the reader thread must wait for actual bytes or a real socket close,
    /// so setting only the shutdown flag (no data, no socket-level shutdown)
    /// leaves it blocked forever and `join` never returns in bounded time.
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
}
