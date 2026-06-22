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

struct AttachConfig {
    session_file: std::path::PathBuf,
    skip_liveness: bool,
}

fn attach<T: TerminalBackend>(config: AttachConfig, mut terminal: T) -> anyhow::Result<()> {
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

/// SSH to [user@]host, find or start a daemon, then attach via a port-forwarding tunnel.
pub fn connect_remote<T: TerminalBackend>(
    target: &str,
    file: Option<String>,
    terminal: T,
) -> anyhow::Result<()> {
    let start_part = match &file {
        Some(f) => format!("rift --daemon --detach {}", shell_escape(f)),
        None => "rift --daemon --detach".to_string(),
    };
    let discover_cmd = format!(
        "bash -lc 'if rift --list-sessions 2>/dev/null; then exit 0; fi; \
         {}; \
         for _i in 1 2 3 4 5 6 7 8 9 10; do \
         if rift --list-sessions 2>/dev/null; then exit 0; fi; \
         sleep 0.5; done; exit 1'",
        start_part.replace('\'', "'\\''"),
    );
    let output = std::process::Command::new("ssh")
        .args([target, &discover_cmd])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    if line.is_empty() {
        anyhow::bail!("no rift session found on {target} and could not start one");
    }
    let info: crate::ipc::session::SessionInfo = serde_json::from_str(line)
        .map_err(|e| anyhow::anyhow!("could not parse session info: {e}"))?;

    let local_port = free_port()?;
    let forward = format!("{}:localhost:{}", local_port, info.port);
    let mut ssh = std::process::Command::new("ssh")
        .args(["-L", &forward, "-N", target])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;
    drop(ssh.stdin.take());

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
    if n > crate::transport::MAX_FRAME_LEN {
        anyhow::bail!(
            "Content-Length {n} exceeds max frame size {}",
            crate::transport::MAX_FRAME_LEN
        );
    }
    let mut body = vec![0u8; n];
    stream.read_exact(&mut body)?;
    Ok(body)
}

fn event_loop<T: TerminalBackend>(local_term: &mut T, stream: TcpStream) -> anyhow::Result<()> {
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

    let _ = write_stream.shutdown(std::net::Shutdown::Both);
    reader_thread.join().ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_one_frame_rejects_oversized_content_length_without_allocating() {
        // The daemon side of the handshake is untrusted at this point (no
        // auth yet); a huge claimed length must not reach `vec![0u8; n]`.
        let mut data: &[u8] = b"Content-Length: 999999999999\r\n\r\n";
        let err = read_one_frame(&mut data).unwrap_err();
        assert!(err.to_string().contains("exceeds max frame size"));
    }
}
