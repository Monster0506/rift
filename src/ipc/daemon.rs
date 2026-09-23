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
        // setsid (async-signal-safe) detaches from the controlling terminal so
        // the daemon survives SIGHUP when the starting SSH session closes.
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
fn publish_shutdown_flag(shutdown: Arc<AtomicBool>) {
    SHUTDOWN_FLAG.store(shutdown.as_ptr() as *mut _, Ordering::Relaxed);
    std::mem::forget(shutdown);
}

#[cfg(unix)]
fn install_sigterm_handler(shutdown: Arc<AtomicBool>) {
    install_sigterm_handler_with(shutdown, || {});
}

/// `after_publish` runs between the flag publish and the OS-level install;
/// tests use it to check the flag is already live there. Production passes a no-op.
#[cfg(unix)]
fn install_sigterm_handler_with(shutdown: Arc<AtomicBool>, after_publish: impl FnOnce()) {
    extern "C" {
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> extern "C" fn(i32);
    }
    // Publish before installing the handler so a signal can never observe a null pointer.
    publish_shutdown_flag(shutdown);
    after_publish();
    let _ = unsafe { signal(15, sigterm_handler) };
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

/// Clamp a peer-supplied dimension into the valid u16 range instead of
/// silently wrapping (e.g. 65536 -> 0) when cast from u64.
fn clamp_dimension(value: u64, default: u16) -> u16 {
    if value == 0 || value > u16::MAX as u64 {
        default
    } else {
        value as u16
    }
}

/// Forward a key, blocking if the channel is full so a burst never drops a
/// keystroke. Returns false if the receiver (editor thread) has hung up.
fn forward_key(input_tx: &SyncSender<Key>, key: Key) -> bool {
    input_tx.send(key).is_ok()
}

fn parse_resize_params(params: &serde_json::Value) -> Key {
    let cols = params["cols"]
        .as_u64()
        .map_or(80, |v| clamp_dimension(v, 80));
    let rows = params["rows"]
        .as_u64()
        .map_or(24, |v| clamp_dimension(v, 24));
    Key::Resize(cols, rows)
}

/// How often the reader checks `shutdown` between blocking reads. Bounds how
/// long `join()` can take after a shutdown is requested.
const READER_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Reads frames from `reader` into `msg_tx` until disconnect or `shutdown`;
/// a read timeout ensures it notices `shutdown` even with no socket close.
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

    // Periodic read timeout lets the reader notice `reader_shutdown` even if
    // the cross-clone socket shutdown below doesn't interrupt it on this platform.
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
                                    if !forward_key(input_tx, key) {
                                        result = ServeResult::EditorExited;
                                        break 'serve;
                                    }
                                }
                            }
                        }
                        "resize" => {
                            // Dropping a stale resize is fine; only the latest size matters.
                            let key = parse_resize_params(&msg["params"]);
                            if input_tx.try_send(key).is_err() {
                                eprintln!("rift: dropped resize message");
                            }
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

        // Editor pressed :q in remote mode, so detach the client and keep the editor alive.
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
#[path = "daemon_tests.rs"]
mod daemon_tests;
