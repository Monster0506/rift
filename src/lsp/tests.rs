use super::mod_fns;
use super::protocol::*;
use super::*;
use std::path::Path;

// ── protocol helpers ──────────────────────────────────────────────────────────

#[test]
fn path_to_uri_unix_style() {
    #[cfg(not(windows))]
    {
        let p = Path::new("/home/user/project/main.rs");
        assert_eq!(path_to_uri(p), "file:///home/user/project/main.rs");
    }
}

#[test]
fn path_to_uri_windows_style() {
    #[cfg(windows)]
    {
        let p = Path::new(r"C:\Users\user\project\main.rs");
        let uri = path_to_uri(p);
        assert!(uri.starts_with("file:///"), "got: {}", uri);
        assert!(uri.contains("main.rs"), "got: {}", uri);
    }
}

#[test]
fn uri_to_path_roundtrip() {
    #[cfg(not(windows))]
    {
        let original = Path::new("/home/user/project/main.rs");
        let uri = path_to_uri(original);
        let recovered = uri_to_path(&uri).expect("should parse");
        assert_eq!(recovered, original);
    }
}

#[test]
fn uri_to_path_decodes_percent_encoded_multibyte_utf8() {
    // "é" encodes to the 2-byte UTF-8 sequence %C3%A9; decoding byte-by-byte
    // as Latin-1 (`byte as char`) would yield "Ã©" instead of "é".
    #[cfg(not(windows))]
    {
        let uri = "file:///home/user/r%C3%A9sum%C3%A9.rs";
        let recovered = uri_to_path(uri).expect("should parse");
        assert_eq!(recovered, Path::new("/home/user/résumé.rs"));
    }
    #[cfg(windows)]
    {
        let uri = "file:///c:/users/test/r%C3%A9sum%C3%A9.rs";
        let recovered = uri_to_path(uri).expect("should parse");
        assert_eq!(recovered, Path::new(r"c:\users\test\résumé.rs"));
    }
}

#[test]
fn utf16_char_offset_round_trip_for_astral_emoji() {
    // "🦀" (U+1F980) is outside the BMP: 1 code point, but 2 UTF-16 code units.
    let line = "a🦀b";
    assert_eq!(char_offset_to_utf16(line.chars(), 0), 0);
    assert_eq!(char_offset_to_utf16(line.chars(), 1), 1); // past 'a'
    assert_eq!(char_offset_to_utf16(line.chars(), 2), 3); // past 'a' + crab (2 units)
    assert_eq!(char_offset_to_utf16(line.chars(), 3), 4); // past 'b'

    assert_eq!(utf16_offset_to_char(line.chars(), 0), 0);
    assert_eq!(utf16_offset_to_char(line.chars(), 1), 1);
    assert_eq!(utf16_offset_to_char(line.chars(), 3), 2);
    assert_eq!(utf16_offset_to_char(line.chars(), 4), 3);
}

#[test]
fn utf8_char_offset_round_trip_for_astral_emoji() {
    // "🦀" is 1 code point but 4 UTF-8 bytes.
    let line = "a🦀b";
    assert_eq!(char_offset_to_utf8(line.chars(), 1), 1); // past 'a'
    assert_eq!(char_offset_to_utf8(line.chars(), 2), 5); // past 'a' + crab (4 bytes)
    assert_eq!(char_offset_to_utf8(line.chars(), 3), 6); // past 'b'

    assert_eq!(utf8_offset_to_char(line.chars(), 1), 1);
    assert_eq!(utf8_offset_to_char(line.chars(), 5), 2);
    assert_eq!(utf8_offset_to_char(line.chars(), 6), 3);
}

#[test]
fn position_encoding_from_wire_parses_known_values_only() {
    assert_eq!(
        PositionEncoding::from_wire("utf-16"),
        Some(PositionEncoding::Utf16)
    );
    assert_eq!(
        PositionEncoding::from_wire("utf-8"),
        Some(PositionEncoding::Utf8)
    );
    assert_eq!(PositionEncoding::from_wire("utf-32"), None);
    assert_eq!(PositionEncoding::default(), PositionEncoding::Utf16);
}

#[test]
fn uri_to_path_invalid_returns_none() {
    assert!(uri_to_path("http://example.com/foo").is_none());
    assert!(uri_to_path("not-a-uri").is_none());
}

// ── JSON-RPC serialisation ────────────────────────────────────────────────────

#[test]
fn json_rpc_request_serialises_correctly() {
    let req = super::protocol::JsonRpcRequest {
        jsonrpc: "2.0",
        id: 42,
        method: "textDocument/hover".into(),
        params: Some(serde_json::json!({ "key": "value" })),
    };
    let s = serde_json::to_string(&req).unwrap();
    assert!(s.contains("\"jsonrpc\":\"2.0\""), "got: {}", s);
    assert!(s.contains("\"id\":42"), "got: {}", s);
    assert!(s.contains("textDocument/hover"), "got: {}", s);
}

#[test]
fn json_rpc_request_omits_null_params() {
    let req = super::protocol::JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "shutdown".into(),
        params: None,
    };
    let s = serde_json::to_string(&req).unwrap();
    assert!(!s.contains("params"), "should omit null params, got: {}", s);
}

// ── LspDiagnostic deserialisation ────────────────────────────────────────────

#[test]
fn diagnostic_deserialises_with_severity() {
    let json = serde_json::json!({
        "range": {
            "start": { "line": 3, "character": 5 },
            "end":   { "line": 3, "character": 12 }
        },
        "severity": 1,
        "message": "use of undeclared variable",
        "source": "rust-analyzer"
    });
    let diag: LspDiagnostic = serde_json::from_value(json).unwrap();
    assert_eq!(diag.range.start.line, 3);
    assert_eq!(diag.range.start.character, 5);
    assert_eq!(diag.severity, Some(1));
    assert_eq!(diag.message, "use of undeclared variable");
    assert_eq!(diag.source, Some("rust-analyzer".into()));
}

#[test]
fn diagnostic_deserialises_without_severity() {
    let json = serde_json::json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end":   { "line": 0, "character": 1 }
        },
        "message": "something"
    });
    let diag: LspDiagnostic = serde_json::from_value(json).unwrap();
    assert_eq!(diag.severity, None);
}

// ── LspLocation deserialisation ───────────────────────────────────────────────

#[test]
fn location_deserialises() {
    let json = serde_json::json!({
        "uri": "file:///project/src/lib.rs",
        "range": {
            "start": { "line": 10, "character": 4 },
            "end":   { "line": 10, "character": 14 }
        }
    });
    let loc: LspLocation = serde_json::from_value(json).unwrap();
    assert_eq!(loc.uri, "file:///project/src/lib.rs");
    assert_eq!(loc.range.start.line, 10);
}

// ── LspTextEdit deserialisation ───────────────────────────────────────────────

#[test]
fn text_edit_deserialises() {
    let json = serde_json::json!({
        "range": {
            "start": { "line": 2, "character": 0 },
            "end":   { "line": 4, "character": 0 }
        },
        "newText": "fn replaced() {}\n"
    });
    let edit: LspTextEdit = serde_json::from_value(json).unwrap();
    assert_eq!(edit.range.start.line, 2);
    assert_eq!(edit.new_text, "fn replaced() {}\n");
}

// ── hover text extraction ─────────────────────────────────────────────────────

#[test]
fn hover_plain_string() {
    let result = serde_json::json!({ "contents": "plain text" });
    let text = mod_fns::extract_hover_text_pub(&result).unwrap();
    assert_eq!(text, "plain text");
}

#[test]
fn hover_marked_string_object() {
    let result = serde_json::json!({
        "contents": { "language": "rust", "value": "fn foo() -> i32" }
    });
    let text = mod_fns::extract_hover_text_pub(&result).unwrap();
    assert_eq!(text, "fn foo() -> i32");
}

#[test]
fn hover_array_of_strings() {
    let result = serde_json::json!({
        "contents": ["first part", "second part"]
    });
    let text = mod_fns::extract_hover_text_pub(&result).unwrap();
    assert!(text.contains("first part"), "got: {}", text);
    assert!(text.contains("second part"), "got: {}", text);
}

#[test]
fn hover_null_result_returns_none() {
    let result = serde_json::json!(null);
    assert!(mod_fns::extract_hover_text_pub(&result).is_none());
}

// ── config ────────────────────────────────────────────────────────────────────

#[test]
fn language_id_mapping() {
    assert_eq!(super::config::language_id("rust"), "rust");
    assert_eq!(super::config::language_id("bash"), "shellscript");
    assert_eq!(super::config::language_id("unknown_lang"), "plaintext");
}

// ── LspManager (no live server) ───────────────────────────────────────────────

#[test]
fn manager_poll_with_no_clients_returns_empty() {
    let mut mgr = LspManager::new(None);
    let msgs = mgr.poll();
    assert!(msgs.is_empty());
}

#[test]
fn manager_did_open_unknown_filetype_is_noop() {
    let mut mgr = LspManager::new(None);
    let p = Path::new("/tmp/file.cobol");
    // Should not panic; server_for_language returns None for "cobol"
    mgr.did_open(p, "cobol", "IDENTIFICATION DIVISION.");
    let msgs = mgr.poll();
    assert!(msgs.is_empty());
}

#[test]
fn manager_did_change_without_open_is_noop() {
    let mut mgr = LspManager::new(None);
    let p = Path::new("/tmp/nope.rs");
    mgr.did_change(p, "fn main() {}");
    // No panic, no messages
    assert!(mgr.poll().is_empty());
}

#[test]
fn manager_goto_definition_without_open_returns_none() {
    let mut mgr = LspManager::new(None);
    let p = Path::new("/tmp/nope.rs");
    assert!(mgr.goto_definition(p, 0, 0).is_none());
}

#[test]
fn manager_has_client_for_path_false_when_not_opened() {
    let mgr = LspManager::new(None);
    assert!(!mgr.has_client_for_path(Path::new("/any/path.rs")));
}

#[test]
fn flycheck_progress_never_gates_requests_and_readiness_is_permanent() {
    let mut mgr = LspManager::new(None);
    let mut out = Vec::new();
    let progress = |token: &str, kind: &str, title: &str| serde_json::json!({ "token": token, "value": { "kind": kind, "title": title } });

    // Real indexing gates until it ends plus the grace period.
    mgr.on_progress(
        "rust",
        &progress("rustAnalyzer/Indexing", "begin", "Indexing"),
        &mut out,
    );
    assert!(mgr.is_indexing("rust"));
    mgr.on_progress(
        "rust",
        &progress("rustAnalyzer/Indexing", "end", ""),
        &mut out,
    );
    assert!(
        mgr.is_indexing("rust"),
        "grace period still counts as indexing"
    );

    // A flycheck token starting inside the grace period must not cancel it, and
    // must not push a status-bar Progress message (it would show stale indexing counts).
    let before = out.len();
    mgr.on_progress(
        "rust",
        &progress("rustAnalyzer/Flycheck/0", "begin", "cargo check"),
        &mut out,
    );
    assert_eq!(
        out.len(),
        before,
        "flycheck begin must not push a Progress message"
    );
    std::thread::sleep(std::time::Duration::from_millis(650));
    let ready = mgr
        .poll()
        .iter()
        .any(|m| matches!(m, LspMessage::ServerReady { language } if language == "rust"));
    assert!(
        ready,
        "ServerReady must fire while only a flycheck token is active"
    );
    assert!(!mgr.is_indexing("rust"));

    // A flycheck end likewise pushes nothing and never gates.
    let before = out.len();
    mgr.on_progress(
        "rust",
        &progress("rustAnalyzer/Flycheck/0", "end", ""),
        &mut out,
    );
    assert_eq!(
        out.len(),
        before,
        "flycheck end must not push a Progress message"
    );
    assert!(!mgr.is_indexing("rust"));

    // After readiness, a later re-index only affects status, never gating.
    // The earlier flycheck begin/end never touched these counters.
    mgr.on_progress(
        "rust",
        &progress("rustAnalyzer/Indexing", "begin", "Indexing"),
        &mut out,
    );
    assert!(!mgr.is_indexing("rust"));
    assert_eq!(mgr.indexing_progress("rust"), Some((1, 2)));
}

#[test]
fn did_open_without_registered_server_does_not_track_the_document() {
    // A doc opened before its server is registered must not be poisoned:
    // a later did_open (after registration) has to be able to attach it.
    let mut mgr = LspManager::new(None);
    let p = Path::new("/tmp/late.cobol");
    mgr.did_open(p, "cobol", "x");
    assert!(!mgr.has_client_for_path(p));
    assert!(mgr.language_for_uri(&path_to_uri(p)).is_none());
}

#[test]
fn path_to_uri_percent_encodes_reserved_characters() {
    #[cfg(not(windows))]
    {
        let p = Path::new("/home/u/my project/a#b?.rs");
        assert_eq!(path_to_uri(p), "file:///home/u/my%20project/a%23b%3F.rs");
        assert_eq!(uri_to_path(&path_to_uri(p)).unwrap(), p);
    }
    #[cfg(windows)]
    {
        let p = Path::new(r"C:\Users\Me\my project\a#b.rs");
        assert_eq!(path_to_uri(p), "file:///c:/Users/Me/my%20project/a%23b.rs");
        assert_eq!(
            uri_to_path(&path_to_uri(p)).unwrap(),
            Path::new(r"c:\Users\Me\my project\a#b.rs")
        );
    }
}

#[test]
fn path_to_uri_keeps_path_case_and_only_folds_the_drive_letter() {
    #[cfg(windows)]
    {
        let uri = path_to_uri(Path::new(r"\\?\C:\Users\Me\Src\Main.rs"));
        assert_eq!(uri, "file:///c:/Users/Me/Src/Main.rs");
    }
}

#[test]
fn normalize_uri_equates_server_and_client_spellings() {
    // rust-analyzer/VS Code style `c%3A` vs our `c:`; Windows folds case too.
    let a = normalize_uri("file:///c%3A/Users/Me/main.rs");
    let b = normalize_uri("file:///c:/Users/Me/main.rs");
    assert_eq!(a, b);
    assert_eq!(normalize_uri("file:///a/b%20c.rs"), "file:///a/b c.rs");
    #[cfg(windows)]
    assert_eq!(
        normalize_uri("file:///C:/X.rs"),
        normalize_uri("file:///c:/x.rs")
    );
}

#[test]
fn uri_to_path_unc_keeps_both_leading_slashes_on_windows() {
    #[cfg(windows)]
    {
        let p = uri_to_path("file:////server/share/x.rs").unwrap();
        assert_eq!(p, Path::new(r"\\server\share\x.rs"));
    }
}

#[test]
fn shutdown_all_clears_clients_and_kills_the_process() {
    use config::LspServerConfig;

    let mut mgr = LspManager::new(None);
    mgr.register_server(
        "fake".to_string(),
        LspServerConfig {
            // A long-running, stdio-redirect-safe placeholder "server".
            command: "ping".to_string(),
            args: vec!["-n".to_string(), "30".to_string(), "127.0.0.1".to_string()],
            extensions: vec![],
            root_markers: vec![],
            capabilities: vec![],
            initialization_options: None,
            keep_alive: false,
        },
    );
    mgr.did_open(Path::new("/tmp/fake.rs"), "fake", "content");
    assert!(
        mgr.clients.contains_key("fake"),
        "client should have spawned"
    );
    let pid = mgr.clients["fake"].pid();

    mgr.shutdown_all();

    assert!(
        mgr.clients.is_empty(),
        "shutdown_all must clear the client map"
    );
    assert!(
        !process_is_running(pid),
        "shutdown_all must not leave the process running"
    );
}

#[test]
fn a_server_that_exits_is_forgotten_and_respawn_stops_after_the_budget() {
    use config::LspServerConfig;

    let mut mgr = LspManager::new(None);
    // A "server" that exits immediately without ever answering initialize.
    let (command, args) = if cfg!(windows) {
        (
            "cmd".to_string(),
            vec!["/c".to_string(), "exit".to_string()],
        )
    } else {
        ("true".to_string(), vec![])
    };
    mgr.register_server(
        "dead".to_string(),
        LspServerConfig {
            command,
            args,
            extensions: vec![],
            root_markers: vec![],
            capabilities: vec![],
            initialization_options: None,
            keep_alive: false,
        },
    );
    let p = Path::new("/tmp/dead.rs");

    for round in 0..MAX_RESTARTS {
        mgr.did_open(p, "dead", "x");
        assert!(
            mgr.has_client_for_path(p),
            "round {round}: doc should be tracked"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut exited = false;
        while std::time::Instant::now() < deadline && !exited {
            exited = mgr
                .poll()
                .iter()
                .any(|m| matches!(m, LspMessage::ServerExited { language } if language == "dead"));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(exited, "round {round}: expected ServerExited");
        assert!(
            !mgr.has_client_for_path(p),
            "exited server must drop its docs"
        );
        assert!(!mgr.clients.contains_key("dead"));
    }

    // Budget exhausted: no respawn, and the doc stays untracked.
    mgr.did_open(p, "dead", "x");
    assert!(!mgr.clients.contains_key("dead"));
    assert!(!mgr.has_client_for_path(p));

    // Re-registering the server resets the budget.
    mgr.register_server(
        "dead".to_string(),
        LspServerConfig {
            command: "ping".to_string(),
            args: vec!["-n".to_string(), "30".to_string(), "127.0.0.1".to_string()],
            extensions: vec![],
            root_markers: vec![],
            capabilities: vec![],
            initialization_options: None,
            keep_alive: false,
        },
    );
    mgr.did_open(p, "dead", "x");
    assert!(mgr.clients.contains_key("dead"));
    mgr.shutdown_all();
}

#[test]
fn did_close_stops_tracking_a_previously_opened_document() {
    use config::LspServerConfig;

    let mut mgr = LspManager::new(None);
    mgr.register_server(
        "fake".to_string(),
        LspServerConfig {
            command: "ping".to_string(),
            args: vec!["-n".to_string(), "30".to_string(), "127.0.0.1".to_string()],
            extensions: vec![],
            root_markers: vec![],
            capabilities: vec![],
            initialization_options: None,
            keep_alive: false,
        },
    );
    let path = Path::new("/tmp/fake_close.rs");
    mgr.did_open(path, "fake", "content");
    assert!(mgr.is_tracking(path), "should be tracked after did_open");

    mgr.did_close(path);
    assert!(
        !mgr.is_tracking(path),
        "did_close must stop tracking the document"
    );

    mgr.shutdown_all();
}

#[test]
fn requests_outside_declared_capabilities_are_rejected() {
    use config::{LspCapability, LspServerConfig};

    let mut mgr = LspManager::new(None);
    mgr.register_server(
        "fake".to_string(),
        LspServerConfig {
            command: "ping".to_string(),
            args: vec!["-n".to_string(), "30".to_string(), "127.0.0.1".to_string()],
            extensions: vec![],
            root_markers: vec![],
            capabilities: vec![LspCapability::Hover],
            initialization_options: None,
            keep_alive: false,
        },
    );
    let path = Path::new("/tmp/fake_capability.rs");
    mgr.did_open(path, "fake", "content");

    assert!(
        mgr.hover(path, 0, 0).is_some(),
        "hover is declared, so it should be allowed"
    );
    assert!(
        mgr.goto_definition(path, 0, 0).is_none(),
        "goto_definition is not declared, so it should be rejected"
    );
    assert!(
        mgr.references(path, 0, 0).is_none(),
        "references is not declared, so it should be rejected"
    );
    assert!(
        mgr.rename(path, 0, 0, "x".to_string()).is_none(),
        "rename is not declared, so it should be rejected"
    );
    assert!(
        mgr.format(path, 4, true).is_none(),
        "format is not declared, so it should be rejected"
    );
    assert!(
        mgr.code_action(path, 0, 0, vec![]).is_none(),
        "code_action is not declared, so it should be rejected"
    );

    mgr.shutdown_all();
}

#[test]
fn a_server_with_no_declared_capabilities_allows_everything() {
    use config::LspServerConfig;

    let mut mgr = LspManager::new(None);
    mgr.register_server(
        "fake".to_string(),
        LspServerConfig {
            command: "ping".to_string(),
            args: vec!["-n".to_string(), "30".to_string(), "127.0.0.1".to_string()],
            extensions: vec![],
            root_markers: vec![],
            capabilities: vec![],
            initialization_options: None,
            keep_alive: false,
        },
    );
    let path = Path::new("/tmp/fake_no_capabilities.rs");
    mgr.did_open(path, "fake", "content");

    assert!(mgr.hover(path, 0, 0).is_some());
    assert!(mgr.goto_definition(path, 0, 0).is_some());

    mgr.shutdown_all();
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

// ── route_response integration (via public mod_fns) ──────────────────────────

#[test]
fn route_definition_response_with_locations() {
    let locations_json = serde_json::json!([
        {
            "uri": "file:///foo/bar.rs",
            "range": {
                "start": { "line": 5, "character": 2 },
                "end":   { "line": 5, "character": 10 }
            }
        }
    ]);
    let msg = super::mod_fns::route_response_pub("textDocument/definition", None, locations_json);
    match msg {
        Some(LspMessage::GotoDefinitionResult { locations, .. }) => {
            assert_eq!(locations.len(), 1);
            assert_eq!(locations[0].range.start.line, 5);
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_definition_response_empty_array() {
    let msg =
        super::mod_fns::route_response_pub("textDocument/definition", None, serde_json::json!([]));
    match msg {
        Some(LspMessage::GotoDefinitionResult { locations, .. }) => {
            assert!(locations.is_empty());
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_definition_response_accepts_location_links() {
    let links = serde_json::json!([{
        "originSelectionRange": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 3 } },
        "targetUri": "file:///foo/def.rs",
        "targetRange": { "start": { "line": 10, "character": 0 }, "end": { "line": 20, "character": 1 } },
        "targetSelectionRange": { "start": { "line": 10, "character": 7 }, "end": { "line": 10, "character": 12 } }
    }]);
    let msg =
        super::mod_fns::route_response_pub("textDocument/definition", Some("file:///x.rs"), links);
    match msg {
        Some(LspMessage::GotoDefinitionResult { locations, uri }) => {
            assert_eq!(uri, "file:///x.rs");
            assert_eq!(locations.len(), 1);
            assert_eq!(locations[0].uri, "file:///foo/def.rs");
            assert_eq!(locations[0].range.start.line, 10);
            assert_eq!(locations[0].range.start.character, 7);
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_definition_response_null_is_an_empty_result() {
    let msg = super::mod_fns::route_response_pub(
        "textDocument/definition",
        None,
        serde_json::Value::Null,
    );
    assert!(
        matches!(msg, Some(LspMessage::GotoDefinitionResult { locations, .. }) if locations.is_empty())
    );
}

#[test]
fn route_notification_carries_diagnostics_version_and_show_message() {
    let params = serde_json::json!({
        "uri": "file:///main.rs", "version": 7, "diagnostics": []
    });
    match mod_fns::route_notification_pub("textDocument/publishDiagnostics", params) {
        Some(LspMessage::Diagnostics { version, .. }) => assert_eq!(version, Some(7)),
        other => panic!("unexpected: {:?}", other),
    }
    let params = serde_json::json!({ "type": 2, "message": "careful" });
    match mod_fns::route_notification_pub("window/showMessage", params) {
        Some(LspMessage::ShowMessage { kind, message }) => {
            assert_eq!(kind, 2);
            assert_eq!(message, "careful");
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_references_response() {
    let json = serde_json::json!([
        {
            "uri": "file:///a.rs",
            "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 5 } }
        },
        {
            "uri": "file:///b.rs",
            "range": { "start": { "line": 20, "character": 3 }, "end": { "line": 20, "character": 8 } }
        }
    ]);
    let msg = super::mod_fns::route_response_pub("textDocument/references", None, json);
    match msg {
        Some(LspMessage::ReferencesResult { locations, .. }) => {
            assert_eq!(locations.len(), 2);
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_hover_response_plain_string() {
    let json = serde_json::json!({ "contents": "i32" });
    let msg = super::mod_fns::route_response_pub("textDocument/hover", None, json);
    match msg {
        Some(LspMessage::HoverResult { contents, .. }) => {
            assert_eq!(contents, "i32");
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_hover_response_null() {
    let msg =
        super::mod_fns::route_response_pub("textDocument/hover", None, serde_json::json!(null));
    match msg {
        Some(LspMessage::HoverResult { contents, .. }) => {
            assert!(
                contents.is_empty(),
                "expected empty hover, got: {}",
                contents
            );
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_formatting_response() {
    let json = serde_json::json!([
        {
            "range": {
                "start": { "line": 0, "character": 0 },
                "end":   { "line": 0, "character": 5 }
            },
            "newText": "hello"
        }
    ]);
    // URI is passed as a separate parameter, not encoded in the method string.
    let msg =
        super::mod_fns::route_response_pub("textDocument/formatting", Some("file:///a.rs"), json);
    match msg {
        Some(LspMessage::FormattingResult { uri, edits }) => {
            assert_eq!(uri, "file:///a.rs");
            assert_eq!(edits.len(), 1);
            assert_eq!(edits[0].new_text, "hello");
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_formatting_response_no_uri_gives_empty() {
    let json = serde_json::json!([{
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
        "newText": "x"
    }]);
    let msg = super::mod_fns::route_response_pub("textDocument/formatting", None, json);
    match msg {
        Some(LspMessage::FormattingResult { uri, .. }) => assert!(uri.is_empty()),
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_code_action_response() {
    let json = serde_json::json!([
        { "title": "Add missing import" },
        { "title": "Fix all errors" }
    ]);
    let msg = super::mod_fns::route_response_pub("textDocument/codeAction", None, json);
    match msg {
        Some(LspMessage::CodeActionResult { actions, .. }) => {
            assert_eq!(actions.len(), 2);
            assert_eq!(actions[0]["title"], "Add missing import");
            assert_eq!(actions[1]["title"], "Fix all errors");
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_notification_diagnostics() {
    let json = serde_json::json!({
        "uri": "file:///main.rs",
        "diagnostics": [
            {
                "range": {
                    "start": { "line": 5, "character": 0 },
                    "end":   { "line": 5, "character": 10 }
                },
                "severity": 1,
                "message": "type mismatch"
            }
        ]
    });
    let msg = super::mod_fns::route_notification_pub("textDocument/publishDiagnostics", json);
    match msg {
        Some(LspMessage::Diagnostics {
            uri, diagnostics, ..
        }) => {
            assert_eq!(uri, "file:///main.rs");
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].message, "type mismatch");
            assert_eq!(diagnostics[0].severity, Some(1));
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_notification_empty_diagnostics() {
    let json = serde_json::json!({
        "uri": "file:///clean.rs",
        "diagnostics": []
    });
    let msg = super::mod_fns::route_notification_pub("textDocument/publishDiagnostics", json);
    match msg {
        Some(LspMessage::Diagnostics { diagnostics, .. }) => {
            assert!(diagnostics.is_empty());
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn route_unknown_notification_returns_none() {
    let msg = super::mod_fns::route_notification_pub("window/logMessage", serde_json::json!({}));
    assert!(msg.is_none());
}

#[test]
fn route_initialize_response_returns_none() {
    // initialize is handled internally (sends initialized notification) — no LspMessage
    let msg = super::mod_fns::route_response_pub(
        "initialize",
        None,
        serde_json::json!({ "capabilities": {} }),
    );
    assert!(msg.is_none());
}

#[test]
fn route_unknown_method_returns_none() {
    let msg = super::mod_fns::route_response_pub("$/cancelRequest", None, serde_json::json!(null));
    assert!(msg.is_none());
}

// ── hover soft-wrap ───────────────────────────────────────────────────────────

#[test]
fn hover_long_line_wraps_within_width() {
    // rust-analyzer often sends 150+ char signatures; they must wrap.
    let long_line = "fn very_long_function_name(arg_one: SomeType, arg_two: AnotherLongType, arg_three: YetAnotherLongType) -> ResultType";
    let wrapped = crate::render::wrap_text(long_line, 60);
    assert!(
        wrapped.len() > 1,
        "long hover line should wrap into multiple lines"
    );
    for line in &wrapped {
        use unicode_width::UnicodeWidthStr;
        assert!(
            UnicodeWidthStr::width(line.as_str()) <= 60,
            "wrapped line exceeds width 60: {:?}",
            line
        );
    }
}

#[test]
fn hover_paragraph_breaks_preserved_after_wrap() {
    // A blank line between paragraphs must survive through wrap_text.
    let content = "First paragraph.\n\nSecond paragraph.";
    let wrapped = crate::render::wrap_text(content, 80);
    assert!(
        wrapped.iter().any(|l| l.is_empty()),
        "paragraph break (blank line) should be preserved; got: {:?}",
        wrapped
    );
}

#[test]
fn hover_short_content_stays_on_one_line() {
    let short = "i32";
    let wrapped = crate::render::wrap_text(short, 80);
    assert_eq!(wrapped, vec!["i32"]);
}

#[test]
fn hover_cjk_content_wraps_by_display_width() {
    // "你好" is 2 CJK chars, display width 4 each -> total 4.
    // At width 3 the word cannot be split further so it stays alone on a line.
    let s = "你好 world";
    let wrapped = crate::render::wrap_text(s, 5);
    // "你好" has display width 4 (fits in 5), "world" has width 5 (fits in 5)
    // -> each word on its own line since 4+1+5 = 10 > 5
    assert_eq!(wrapped.len(), 2, "got: {:?}", wrapped);
}
