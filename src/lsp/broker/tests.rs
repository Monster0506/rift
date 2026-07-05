use super::*;

fn req(id: u64, method: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {}})
}

fn resp(id: u64, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn attached_state() -> ProxyState {
    let mut s = ProxyState::default();
    s.on_client_attach();
    s
}

#[test]
fn broker_key_is_stable_and_distinguishes_inputs() {
    let a = broker_key("rust-analyzer", &[], Some("file:///proj"));
    let b = broker_key("rust-analyzer", &[], Some("file:///proj"));
    assert_eq!(a, b, "same inputs must map to the same key");

    let other_root = broker_key("rust-analyzer", &[], Some("file:///other"));
    assert_ne!(a, other_root, "different roots need different brokers");

    let other_args = broker_key("rust-analyzer", &["--log".into()], Some("file:///proj"));
    assert_ne!(a, other_args, "different args need different brokers");

    assert!(a.starts_with("rust-analyzer-"), "key is debuggable: {a}");
}

#[test]
fn client_request_ids_are_remapped_and_mapped_back() {
    let mut s = attached_state();

    let actions = s.on_client(req(7, "textDocument/hover"));
    let Action::ToServer(fwd) = &actions[0] else {
        panic!("expected forward")
    };
    let bid = fwd["id"].as_u64().unwrap();

    let actions = s.on_server(resp(bid, json!({"contents": "hi"})));
    assert_eq!(
        actions,
        vec![Action::ToClient(resp(7, json!({"contents": "hi"})))],
        "response must carry the client's original id"
    );
}

#[test]
fn second_initialize_is_answered_from_cache_without_touching_the_server() {
    let mut s = attached_state();

    // First client initializes normally.
    let actions = s.on_client(req(1, "initialize"));
    let Action::ToServer(fwd) = &actions[0] else {
        panic!("expected forward")
    };
    let bid = fwd["id"].as_u64().unwrap();
    s.on_server(resp(bid, json!({"capabilities": {"hoverProvider": true}})));

    // It detaches; a new session attaches and initializes again.
    s.on_client_detach();
    s.on_client_attach();
    let actions = s.on_client(req(1, "initialize"));
    assert_eq!(
        actions,
        vec![Action::ToClient(resp(
            1,
            json!({"capabilities": {"hoverProvider": true}})
        ))],
        "cached initialize result, nothing sent to the server"
    );
}

#[test]
fn initialize_queued_while_first_is_in_flight_gets_answered_on_arrival() {
    let mut s = attached_state();
    let actions = s.on_client(req(1, "initialize"));
    let Action::ToServer(fwd) = &actions[0] else {
        panic!("expected forward")
    };
    let bid = fwd["id"].as_u64().unwrap();

    // First client dies before the response; a second one initializes.
    s.on_client_detach();
    s.on_client_attach();
    assert_eq!(s.on_client(req(9, "initialize")), vec![], "queued");

    let actions = s.on_server(resp(bid, json!({"capabilities": {}})));
    assert_eq!(
        actions,
        vec![Action::ToClient(resp(9, json!({"capabilities": {}})))],
        "queued initialize answered once the first response lands"
    );
}

#[test]
fn shutdown_and_exit_never_reach_the_server() {
    let mut s = attached_state();

    let actions = s.on_client(req(5, "shutdown"));
    assert_eq!(
        actions,
        vec![Action::ToClient(resp(5, Value::Null))],
        "shutdown is answered locally"
    );

    let actions = s.on_client(json!({"jsonrpc": "2.0", "method": "exit"}));
    assert_eq!(actions, vec![], "exit is swallowed");
}

#[test]
fn repeat_initialized_notifications_are_swallowed() {
    let mut s = attached_state();
    let first = s.on_client(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    assert_eq!(first.len(), 1, "first initialized forwards");

    s.on_client_detach();
    s.on_client_attach();
    let second = s.on_client(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    assert_eq!(
        second,
        vec![],
        "reattaching client's initialized is dropped"
    );
}

#[test]
fn detach_closes_open_docs_and_answers_outstanding_server_requests() {
    let mut s = attached_state();
    s.on_client(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": {"textDocument": {"uri": "file:///a.rs", "text": "", "version": 1}}
    }));
    s.on_server(json!({
        "jsonrpc": "2.0", "id": 42, "method": "workspace/configuration", "params": {}
    }));

    let actions = s.on_client_detach();
    assert!(
        actions.contains(&Action::ToServer(json!({
            "jsonrpc": "2.0", "method": "textDocument/didClose",
            "params": {"textDocument": {"uri": "file:///a.rs"}}
        }))),
        "open doc must be closed on the server: {actions:?}"
    );
    assert!(
        actions.contains(&Action::ToServer(resp(42, Value::Null))),
        "outstanding server request must be unblocked: {actions:?}"
    );
}

#[test]
fn did_close_from_the_client_untracks_the_doc() {
    let mut s = attached_state();
    let open = json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": {"textDocument": {"uri": "file:///a.rs", "text": "", "version": 1}}
    });
    let close = json!({
        "jsonrpc": "2.0", "method": "textDocument/didClose",
        "params": {"textDocument": {"uri": "file:///a.rs"}}
    });
    s.on_client(open);
    s.on_client(close);
    assert_eq!(
        s.on_client_detach(),
        vec![],
        "nothing left to clean up after an explicit didClose"
    );
}

#[test]
fn responses_for_a_dead_client_generation_are_dropped() {
    let mut s = attached_state();
    let actions = s.on_client(req(3, "textDocument/hover"));
    let Action::ToServer(fwd) = &actions[0] else {
        panic!("expected forward")
    };
    let bid = fwd["id"].as_u64().unwrap();

    s.on_client_detach();
    s.on_client_attach();
    assert_eq!(
        s.on_server(resp(bid, json!("late"))),
        vec![],
        "a late response must not reach the wrong client"
    );
}

#[test]
fn server_traffic_while_detached_is_dropped_or_answered() {
    let mut s = ProxyState::default();

    let notif =
        json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {}});
    assert_eq!(s.on_server(notif), vec![], "notification dropped");

    let actions = s.on_server(json!({
        "jsonrpc": "2.0", "id": 8, "method": "window/workDoneProgress/create", "params": {}
    }));
    assert_eq!(
        actions,
        vec![Action::ToServer(resp(8, Value::Null))],
        "server request answered null so it does not hang"
    );
}

#[test]
fn serve_acks_the_right_token_and_rejects_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let key = "test-serve";

    // A placeholder "server" that blocks on stdin and keeps stdout silent;
    // anything chatty (like ping) would read as a broken LSP stream.
    #[cfg(windows)]
    let request = BrokerRequest {
        command: "findstr".to_string(),
        args: vec!["x".to_string()],
    };
    #[cfg(not(windows))]
    let request = BrokerRequest {
        command: "grep".to_string(),
        args: vec!["x".to_string()],
    };
    std::fs::write(
        dir.path().join(format!("{key}.req.json")),
        serde_json::to_string(&request).unwrap(),
    )
    .unwrap();

    let params = BrokerParams {
        key: key.to_string(),
        dir: dir.path().to_path_buf(),
        idle_timeout: Duration::from_millis(600),
    };
    let handle = std::thread::spawn(move || serve(params));

    let desc_path = dir.path().join(format!("{key}.json"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let desc: BrokerDescriptor = loop {
        if let Ok(text) = std::fs::read_to_string(&desc_path) {
            if let Ok(d) = serde_json::from_str(&text) {
                break d;
            }
        }
        if handle.is_finished() {
            panic!("serve exited early: {:?}", handle.join());
        }
        assert!(Instant::now() < deadline, "descriptor never appeared");
        std::thread::sleep(Duration::from_millis(25));
    };

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], desc.port));

    // Wrong token: no ack, connection closed.
    let bad = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).unwrap();
    let mut bad_w = bad.try_clone().unwrap();
    write_framed(
        &mut bad_w,
        &json!({"jsonrpc": "2.0", "method": "broker/hello", "params": {"token": "nope"}}),
    )
    .unwrap();
    bad.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    assert!(
        read_framed(&mut BufReader::new(bad)).is_err(),
        "wrong token must not be acked"
    );

    // Right token: acked.
    let good = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).unwrap();
    let mut good_w = good.try_clone().unwrap();
    write_framed(
        &mut good_w,
        &json!({"jsonrpc": "2.0", "method": "broker/hello", "params": {"token": desc.token}}),
    )
    .unwrap();
    good.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let ack = read_framed(&mut BufReader::new(good.try_clone().unwrap())).unwrap();
    let ack: Value = serde_json::from_slice(&ack).unwrap();
    assert_eq!(ack["method"], "broker/ok");

    // While one client is attached, a second connection is refused (closed).
    let busy = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).unwrap();
    busy.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    assert!(
        read_framed(&mut BufReader::new(busy)).is_err(),
        "second concurrent client must be dropped"
    );

    // Disconnect; the broker idles out, kills the server, removes the file.
    drop(good);
    drop(good_w);
    handle.join().unwrap().unwrap();
    assert!(
        !desc_path.exists(),
        "descriptor must be removed when the broker exits"
    );
}
