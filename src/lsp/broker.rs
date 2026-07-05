//! LSP keepalive broker: a detached `rift --lsp-broker <key>` process owns a
//! language server and proxies it over localhost TCP, one editor at a time.

use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{ChildStdin, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::transport::{read_framed, write_framed};

/// Spawn parameters the editor writes for a broker it is about to launch.
#[derive(Debug, Serialize, Deserialize)]
struct BrokerRequest {
    command: String,
    args: Vec<String>,
}

/// Connection info a running broker publishes for editors to attach to.
#[derive(Debug, Serialize, Deserialize)]
struct BrokerDescriptor {
    pid: u32,
    port: u16,
    token: String,
}

pub fn broker_dir() -> PathBuf {
    crate::ipc::session::data_dir().join("lsp")
}

/// Stable cache key for one (server command, args, project root) combination.
pub fn broker_key(command: &str, args: &[String], root_uri: Option<&str>) -> String {
    // FNV-1a: deterministic across processes and rift builds.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    eat(command.as_bytes());
    for a in args {
        eat(a.as_bytes());
    }
    eat(root_uri.unwrap_or("").as_bytes());

    let stem: String = std::path::Path::new(command)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lsp")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("{}-{:016x}", stem, h)
}

// --- Editor-side attach ---

/// Connect to the keepalive broker for this server, spawning one if needed;
/// the editor exiting only drops the socket, so the server stays warm.
pub fn attach(
    command: &str,
    args: &[String],
    root_uri: Option<&str>,
) -> anyhow::Result<(BufReader<TcpStream>, TcpStream)> {
    if std::env::var_os("RIFT_LSP_NO_KEEPALIVE").is_some() {
        anyhow::bail!("keepalive disabled by RIFT_LSP_NO_KEEPALIVE");
    }
    let dir = broker_dir();
    let key = broker_key(command, args, root_uri);
    let desc_path = dir.join(format!("{key}.json"));

    if let Ok(conn) = try_connect(&desc_path) {
        return Ok(conn);
    }

    std::fs::create_dir_all(&dir)?;
    let _ = std::fs::remove_file(&desc_path);
    let req = BrokerRequest {
        command: command.to_string(),
        args: args.to_vec(),
    };
    std::fs::write(
        dir.join(format!("{key}.req.json")),
        serde_json::to_string(&req)?,
    )?;
    spawn_detached_broker(&key)?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(conn) = try_connect(&desc_path) {
            return Ok(conn);
        }
        if Instant::now() > deadline {
            anyhow::bail!("lsp broker for '{command}' did not come up");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Try one descriptor: TCP connect, token hello, wait for the ok ack.
fn try_connect(desc_path: &std::path::Path) -> anyhow::Result<(BufReader<TcpStream>, TcpStream)> {
    let desc: BrokerDescriptor = serde_json::from_str(&std::fs::read_to_string(desc_path)?)?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], desc.port));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300))?;
    let _ = stream.set_nodelay(true);

    let mut write_half = stream.try_clone()?;
    write_framed(
        &mut write_half,
        &json!({"jsonrpc": "2.0", "method": "broker/hello", "params": {"token": desc.token}}),
    )?;

    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream);
    let ack = read_framed(&mut reader)?;
    let ack: Value = serde_json::from_slice(&ack)?;
    if ack.get("method").and_then(|m| m.as_str()) != Some("broker/ok") {
        anyhow::bail!("broker refused connection");
    }
    write_half.set_read_timeout(None)?;
    Ok((reader, write_half))
}

fn spawn_detached_broker(key: &str) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        extern "C" {
            fn setsid() -> i32;
        }
        // Safety: setsid is async-signal-safe; a new session detaches the
        // broker from the terminal so it survives the editor and SIGHUP.
        unsafe {
            std::process::Command::new(&exe)
                .args(["--lsp-broker", key])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
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
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        std::process::Command::new(&exe)
            .args(["--lsp-broker", key])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
            .spawn()?;
    }

    Ok(())
}

// --- Proxy core (pure, unit-tested) ---

#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    ToServer(Value),
    ToClient(Value),
}

#[derive(Debug)]
struct Inflight {
    generation: u64,
    client_id: Value,
    method: String,
}

/// Message-level proxy state: remaps request ids so sequential clients can
/// reuse them, caches `initialize`, and cleans up when a client detaches.
#[derive(Default)]
pub(crate) struct ProxyState {
    init_result: Option<Value>,
    init_forwarded: bool,
    initialized_sent: bool,
    /// Clients whose `initialize` arrived while the first one is in flight.
    queued_init_ids: Vec<Value>,
    next_broker_id: u64,
    inflight: HashMap<u64, Inflight>,
    /// URIs currently open on the server, so a detach can close them.
    open_docs: HashSet<String>,
    /// Ids of server-to-client requests awaiting a client response.
    outstanding_server_reqs: Vec<Value>,
    generation: u64,
    client_attached: bool,
}

impl ProxyState {
    pub(crate) fn on_client_attach(&mut self) {
        self.client_attached = true;
    }

    /// A client vanished: close its docs on the server and answer any
    /// server-to-client requests with null so the server does not hang.
    pub(crate) fn on_client_detach(&mut self) -> Vec<Action> {
        self.client_attached = false;
        self.generation += 1;
        self.queued_init_ids.clear();
        let mut actions = Vec::new();
        for uri in std::mem::take(&mut self.open_docs) {
            actions.push(Action::ToServer(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didClose",
                "params": {"textDocument": {"uri": uri}}
            })));
        }
        for id in std::mem::take(&mut self.outstanding_server_reqs) {
            actions.push(Action::ToServer(
                json!({"jsonrpc": "2.0", "id": id, "result": null}),
            ));
        }
        actions
    }

    pub(crate) fn on_client(&mut self, mut msg: Value) -> Vec<Action> {
        let method = msg
            .get("method")
            .and_then(|m| m.as_str())
            .map(str::to_string);
        let id = msg.get("id").cloned();
        match (method, id) {
            (Some(method), Some(cid)) => {
                if method == "initialize" {
                    if let Some(result) = &self.init_result {
                        return vec![Action::ToClient(
                            json!({"jsonrpc": "2.0", "id": cid, "result": result.clone()}),
                        )];
                    }
                    if self.init_forwarded {
                        // First initialize still in flight; answer when it lands.
                        self.queued_init_ids.push(cid);
                        return vec![];
                    }
                    self.init_forwarded = true;
                }
                if method == "shutdown" {
                    // The server must outlive this client; pretend it shut down.
                    return vec![Action::ToClient(
                        json!({"jsonrpc": "2.0", "id": cid, "result": null}),
                    )];
                }
                let bid = self.next_broker_id;
                self.next_broker_id += 1;
                self.inflight.insert(
                    bid,
                    Inflight {
                        generation: self.generation,
                        client_id: cid,
                        method,
                    },
                );
                msg["id"] = json!(bid);
                vec![Action::ToServer(msg)]
            }
            (Some(method), None) => match method.as_str() {
                "initialized" => {
                    if self.initialized_sent {
                        vec![]
                    } else {
                        self.initialized_sent = true;
                        vec![Action::ToServer(msg)]
                    }
                }
                "exit" => vec![],
                "textDocument/didOpen" => {
                    if let Some(uri) = msg
                        .pointer("/params/textDocument/uri")
                        .and_then(|u| u.as_str())
                    {
                        self.open_docs.insert(uri.to_string());
                    }
                    vec![Action::ToServer(msg)]
                }
                "textDocument/didClose" => {
                    if let Some(uri) = msg
                        .pointer("/params/textDocument/uri")
                        .and_then(|u| u.as_str())
                    {
                        self.open_docs.remove(uri);
                    }
                    vec![Action::ToServer(msg)]
                }
                _ => vec![Action::ToServer(msg)],
            },
            (None, Some(id)) => {
                // Client response to a server-initiated request.
                self.outstanding_server_reqs.retain(|x| *x != id);
                vec![Action::ToServer(msg)]
            }
            (None, None) => vec![],
        }
    }

    pub(crate) fn on_server(&mut self, mut msg: Value) -> Vec<Action> {
        let has_method = msg.get("method").and_then(|m| m.as_str()).is_some();
        let id = msg.get("id").cloned();
        match (has_method, id) {
            (true, Some(sid)) => {
                if self.client_attached {
                    self.outstanding_server_reqs.push(sid);
                    vec![Action::ToClient(msg)]
                } else {
                    // No one to ask; unblock the server with a null response.
                    vec![Action::ToServer(
                        json!({"jsonrpc": "2.0", "id": sid, "result": null}),
                    )]
                }
            }
            (true, None) => {
                if self.client_attached {
                    vec![Action::ToClient(msg)]
                } else {
                    vec![]
                }
            }
            (false, Some(idv)) => {
                let Some(bid) = idv.as_u64() else {
                    return vec![];
                };
                let Some(inflight) = self.inflight.remove(&bid) else {
                    return vec![];
                };
                let mut actions = Vec::new();
                if inflight.method == "initialize" && self.init_result.is_none() {
                    if let Some(result) = msg.get("result") {
                        self.init_result = Some(result.clone());
                        if self.client_attached {
                            for cid in std::mem::take(&mut self.queued_init_ids) {
                                actions.push(Action::ToClient(json!({
                                    "jsonrpc": "2.0", "id": cid, "result": result.clone()
                                })));
                            }
                        }
                    }
                }
                if self.client_attached && inflight.generation == self.generation {
                    msg["id"] = inflight.client_id;
                    actions.push(Action::ToClient(msg));
                }
                actions
            }
            (false, None) => vec![],
        }
    }
}

// --- Broker runtime ---

pub struct BrokerParams {
    pub key: String,
    pub dir: PathBuf,
    pub idle_timeout: Duration,
}

/// Entry point for `rift --lsp-broker <key>`. Never returns.
pub fn run(key: &str) -> ! {
    let idle_secs = std::env::var("RIFT_LSP_BROKER_IDLE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 60);
    let params = BrokerParams {
        key: key.to_string(),
        dir: broker_dir(),
        idle_timeout: Duration::from_secs(idle_secs),
    };
    match serve(params) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("rift lsp broker: {e}");
            std::process::exit(1);
        }
    }
}

enum Ev {
    NewClient(TcpStream),
    Client(u64, Vec<u8>),
    ClientGone(u64),
    Server(Vec<u8>),
    ServerGone,
}

pub fn serve(params: BrokerParams) -> anyhow::Result<()> {
    let req_path = params.dir.join(format!("{}.req.json", params.key));
    let desc_path = params.dir.join(format!("{}.json", params.key));
    let req: BrokerRequest = serde_json::from_str(&std::fs::read_to_string(&req_path)?)?;
    let _ = std::fs::remove_file(&req_path);

    let mut child = std::process::Command::new(&req.command)
        .args(&req.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut server_stdin = child.stdin.take().expect("stdin");
    let server_stdout = child.stdout.take().expect("stdout");

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let token = crate::ipc::session::generate_token();
    std::fs::create_dir_all(&params.dir)?;
    std::fs::write(
        &desc_path,
        serde_json::to_string(&BrokerDescriptor {
            pid: std::process::id(),
            port,
            token: token.clone(),
        })?,
    )?;
    // If a concurrent broker for the same key won the descriptor race, defer.
    if let Ok(back) = std::fs::read_to_string(&desc_path) {
        if let Ok(d) = serde_json::from_str::<BrokerDescriptor>(&back) {
            if d.pid != std::process::id() {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(());
            }
        }
    }

    let (tx, rx) = mpsc::channel::<Ev>();

    let tx_srv = tx.clone();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(server_stdout);
        loop {
            match read_framed(&mut reader) {
                Ok(body) => {
                    if tx_srv.send(Ev::Server(body)).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx_srv.send(Ev::ServerGone);
                    return;
                }
            }
        }
    });

    let tx_acc = tx.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            if tx_acc.send(Ev::NewClient(stream)).is_err() {
                return;
            }
        }
    });

    let mut state = ProxyState::default();
    let mut active_writer: Option<TcpStream> = None;
    let mut cur_gen: u64 = 0;
    let mut idle_since = Instant::now();

    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ev::NewClient(stream)) => {
                if active_writer.is_some() {
                    continue; // busy: drop; the editor falls back to stdio
                }
                if let Some((writer, reader)) = handshake(stream, &token) {
                    cur_gen += 1;
                    let gen = cur_gen;
                    let tx_cli = tx.clone();
                    std::thread::spawn(move || {
                        let mut reader = reader;
                        loop {
                            match read_framed(&mut reader) {
                                Ok(body) => {
                                    if tx_cli.send(Ev::Client(gen, body)).is_err() {
                                        return;
                                    }
                                }
                                Err(_) => {
                                    let _ = tx_cli.send(Ev::ClientGone(gen));
                                    return;
                                }
                            }
                        }
                    });
                    active_writer = Some(writer);
                    state.on_client_attach();
                }
            }
            Ok(Ev::Client(gen, body)) => {
                if gen != cur_gen {
                    continue;
                }
                if let Ok(msg) = serde_json::from_slice::<Value>(&body) {
                    let actions = state.on_client(msg);
                    apply_actions(actions, &mut server_stdin, &mut active_writer);
                }
            }
            Ok(Ev::ClientGone(gen)) => {
                if gen != cur_gen {
                    continue;
                }
                active_writer = None;
                let actions = state.on_client_detach();
                apply_actions(actions, &mut server_stdin, &mut active_writer);
                idle_since = Instant::now();
            }
            Ok(Ev::Server(body)) => {
                if let Ok(msg) = serde_json::from_slice::<Value>(&body) {
                    let actions = state.on_server(msg);
                    apply_actions(actions, &mut server_stdin, &mut active_writer);
                }
            }
            Ok(Ev::ServerGone) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if active_writer.is_none() && idle_since.elapsed() > params.idle_timeout {
                    let _ = write_framed(
                        &mut server_stdin,
                        &json!({"jsonrpc": "2.0", "id": u32::MAX, "method": "shutdown"}),
                    );
                    let _ = write_framed(
                        &mut server_stdin,
                        &json!({"jsonrpc": "2.0", "method": "exit"}),
                    );
                    std::thread::sleep(Duration::from_millis(200));
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = std::fs::remove_file(&desc_path);
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

/// Validate a connecting client's token and ack it. Returns the write half
/// and the buffered read half, or None to reject.
fn handshake(stream: TcpStream, token: &str) -> Option<(TcpStream, BufReader<TcpStream>)> {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut writer = stream.try_clone().ok()?;
    let mut reader = BufReader::new(stream);

    let hello = read_framed(&mut reader).ok()?;
    let hello: Value = serde_json::from_slice(&hello).ok()?;
    let ok = hello.get("method").and_then(|m| m.as_str()) == Some("broker/hello")
        && hello.pointer("/params/token").and_then(|t| t.as_str()) == Some(token);
    if !ok {
        return None;
    }
    let _ = writer.set_read_timeout(None);
    write_framed(
        &mut writer,
        &json!({"jsonrpc": "2.0", "method": "broker/ok"}),
    )
    .ok()?;
    Some((writer, reader))
}

fn apply_actions(
    actions: Vec<Action>,
    server_stdin: &mut ChildStdin,
    active_writer: &mut Option<TcpStream>,
) {
    for action in actions {
        match action {
            Action::ToServer(msg) => {
                let _ = write_framed(server_stdin, &msg);
            }
            Action::ToClient(msg) => {
                let failed = match active_writer.as_mut() {
                    Some(w) => write_framed(w, &msg).is_err(),
                    None => false,
                };
                // Reader thread will follow up with ClientGone for cleanup.
                if failed {
                    *active_writer = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
