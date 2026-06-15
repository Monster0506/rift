use std::io::Read;
use std::net::TcpStream;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use crate::ipc::key_notation::vim_to_key_sendable;
use crate::ipc::protocol::{
    b64_decode, InitializeParams, InputKeyParams, RenderUpdateParams, ResizeParams,
    SessionEndingParams, Viewport,
};
use crate::key::Key;
use crate::term::TerminalBackend;
use crate::transport::{read_framed, write_framed};

pub struct AttachConfig {
    pub session_file: std::path::PathBuf,
    /// Skip the local is_alive check (used for remote sessions where the pid is foreign).
    pub skip_liveness: bool,
}

pub fn attach<T: TerminalBackend>(config: AttachConfig, mut terminal: T) -> anyhow::Result<()> {
    let info = crate::ipc::session::read(&config.session_file)?;

    if !config.skip_liveness && !crate::ipc::session::is_alive(info.pid) {
        anyhow::bail!(
            "rift daemon (pid {}) is not running -- stale session file at {}",
            info.pid,
            config.session_file.display()
        );
    }

    // 0.0.0.0 is a valid bind address but not a valid connect address on Windows.
    let host = if info.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &info.host
    };
    let addr = format!("{}:{}", host, info.port);

    // Retry for up to 5 s to handle SSH tunnel establishment delay.
    let stream = {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match TcpStream::connect(&addr) {
                Ok(s) => break s,
                Err(e)
                    if e.kind() == std::io::ErrorKind::ConnectionRefused
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => anyhow::bail!("could not connect to daemon: {e}"),
            }
        }
    };

    let size = terminal.get_size().map_err(|e| anyhow::anyhow!(e))?;

    let mut stream = stream;
    let init_msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": InitializeParams {
            token: info.token.clone(),
            client_name: "rift-attach".into(),
            viewport: Viewport { rows: size.rows, cols: size.cols },
        },
    });
    write_framed(&mut stream, &init_msg)?;
    let _init_response = read_one_frame(&mut stream)?;

    terminal.init().map_err(|e| anyhow::anyhow!(e))?;
    let result = event_loop(&mut terminal, stream);
    terminal.deinit();
    result
}

/// SSH to [user@]host, run `rift --list-sessions`, parse the result, then attach.
/// If `start` is true, connects to an existing session or starts a daemon if none found.
/// A single SSH connection handles both the port forward and the session query.
pub fn connect_remote<T: TerminalBackend>(
    target: &str,
    start: bool,
    file: Option<String>,
    port: u16,
    terminal: T,
) -> anyhow::Result<()> {
    let user_host = target.to_string();
    let local_port = free_port()?;
    let forward = format!("{}:localhost:{}", local_port, port);

    // One SSH connection: sets up the port forward, optionally starts the daemon,
    // runs --list-sessions (prints JSON to stdout), then sleeps to keep the tunnel alive.
    // sleep doesn't read stdin, so there is no contention with the editor's input loop.
    let remote_cmd = if start {
        let start_part = match &file {
            Some(f) => format!("rift --daemon --detach {}", shell_escape(f)),
            None => "rift --daemon --detach".to_string(),
        };
        // Use existing session if one is running; only start daemon if none found.
        // Poll up to 5 s for the session file, then sleep to hold the tunnel.
        format!(
            "bash -lc 'rift --list-sessions 2>/dev/null || {}; \
             for _i in 1 2 3 4 5 6 7 8 9 10; do \
             rift --list-sessions 2>/dev/null && sleep 99999; sleep 0.5; done'",
            start_part.replace('\'', "'\\''"),
        )
    } else {
        "bash -lc 'rift --list-sessions && sleep 99999'".to_string()
    };

    let mut ssh = std::process::Command::new("ssh")
        .args(["-L", &forward, &user_host, &remote_cmd])
        // Piped (not inherited) so the SSH process does not compete with the editor's
        // crossterm event loop for terminal input. SSH reads password prompts via
        // /dev/tty directly, so interactive auth still works.
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;
    drop(ssh.stdin.take());

    // Read the one JSON line --list-sessions writes before sleeping.
    let ssh_stdout = ssh.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(ssh_stdout);
    let mut line = String::new();
    use std::io::BufRead;
    reader.read_line(&mut line)?;
    drop(reader);
    let line = line.trim().to_string();

    if line.is_empty() {
        ssh.kill().ok();
        if start {
            anyhow::bail!("failed to start rift daemon on {target}");
        } else {
            anyhow::bail!("no rift session found on {target}");
        }
    }

    let info: crate::ipc::session::SessionInfo = serde_json::from_str(&line).map_err(|e| {
        ssh.kill().ok();
        anyhow::anyhow!("could not parse session info: {e}")
    })?;

    if info.port != port {
        ssh.kill().ok();
        anyhow::bail!(
            "remote daemon is on port {} but forwarding port {}; pass --port {}",
            info.port,
            port,
            info.port
        );
    }

    eprintln!(
        "rift: attaching via tunnel (pid {}, port {})",
        info.pid, local_port
    );

    let mut tunneled = info;
    tunneled.host = "127.0.0.1".into();
    tunneled.port = local_port;

    let tmp = std::env::temp_dir().join(format!("rift-remote-{}.json", tunneled.pid));
    crate::ipc::session::write(&tunneled, &tmp)?;
    let cfg = AttachConfig {
        session_file: tmp.clone(),
        skip_liveness: true,
    };
    let result = attach(cfg, terminal);
    ssh.kill().ok();
    let _ = std::fs::remove_file(&tmp);

    result
}

fn free_port() -> anyhow::Result<u16> {
    // Binds port 0 to get an OS-assigned port, then releases it for SSH to bind.
    // There is an inherent TOCTOU race here; the window is small in practice.
    let l = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

/// Single-quote escape a string for POSIX shell (bash -lc "...").
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn read_one_frame(stream: &mut impl Read) -> anyhow::Result<Vec<u8>> {
    let mut header_buf = Vec::new();
    let mut one = [0u8; 1];
    let mut content_length: Option<usize> = None;

    loop {
        stream.read_exact(&mut one)?;
        header_buf.push(one[0]);
        let n = header_buf.len();
        if n >= 4 && &header_buf[n - 4..] == b"\r\n\r\n" {
            break;
        }
    }

    let header_str = std::str::from_utf8(&header_buf)?;
    for line in header_str.split("\r\n") {
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                content_length = Some(n);
            }
        }
    }

    let n = content_length.ok_or_else(|| anyhow::anyhow!("no Content-Length in handshake"))?;
    let mut body = vec![0u8; n];
    stream.read_exact(&mut body)?;
    Ok(body)
}

fn event_loop<T: TerminalBackend>(local_term: &mut T, stream: TcpStream) -> anyhow::Result<()> {
    // Network reads run on a dedicated thread with no timeout, so a slow or large
    // frame never causes partial-header corruption.
    let mut write_stream = stream.try_clone()?;
    let (frame_tx, frame_rx) = std::sync::mpsc::channel::<std::io::Result<Vec<u8>>>();
    let reader_thread = std::thread::spawn(move || {
        let mut net_reader = std::io::BufReader::new(stream);
        loop {
            match read_framed(&mut net_reader) {
                Ok(bytes) => {
                    if frame_tx.send(Ok(bytes)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = frame_tx.send(Err(e));
                    break;
                }
            }
        }
    });

    let one_ms = Duration::from_millis(1);
    let mut seq: u64 = 0;

    loop {
        if local_term.poll(one_ms).map_err(|e| anyhow::anyhow!(e))? {
            if let Some(key) = local_term.read_key().map_err(|e| anyhow::anyhow!(e))? {
                match key {
                    Key::Resize(cols, rows) => {
                        let msg = serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": "resize",
                            "params": ResizeParams { rows, cols },
                        });
                        write_framed(&mut write_stream, &msg)?;
                    }
                    other => {
                        if let Some(notation) = vim_to_key_sendable(other) {
                            seq += 1;
                            let msg = serde_json::json!({
                                "jsonrpc": "2.0",
                                "method": "input.key",
                                "params": InputKeyParams { key: notation, seq },
                            });
                            write_framed(&mut write_stream, &msg)?;
                        }
                    }
                }
            }
        }

        match frame_rx.try_recv() {
            Ok(Ok(bytes)) => {
                let msg: serde_json::Value = serde_json::from_slice(&bytes)?;
                let method = msg["method"].as_str().unwrap_or("");

                if method == "render.update" {
                    let params: RenderUpdateParams = serde_json::from_value(msg["params"].clone())?;
                    if let Some(screen_bytes) = b64_decode(&params.screen) {
                        local_term
                            .write(&screen_bytes)
                            .map_err(|e| anyhow::anyhow!(e))?;
                        local_term.flush().map_err(|e| anyhow::anyhow!(e))?;
                    }
                } else if method == "session.ending" {
                    let params: SessionEndingParams =
                        serde_json::from_value(msg["params"].clone())?;
                    eprintln!("rift session ended: {}", params.reason);
                    break;
                }
            }
            Ok(Err(e)) => {
                eprintln!("rift: disconnected unexpectedly ({e})");
                break;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                eprintln!("rift: disconnected unexpectedly");
                break;
            }
        }
    }

    // Shut down the socket to unblock the reader thread, then wait for it to exit.
    let _ = write_stream.shutdown(std::net::Shutdown::Both);
    reader_thread.join().ok();
    Ok(())
}
