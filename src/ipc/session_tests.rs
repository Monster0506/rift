use super::*;
use tempfile::TempDir;

#[test]
fn write_and_read_session() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("12345.json");
    let info = SessionInfo {
        pid: 12345,
        host: "127.0.0.1".into(),
        port: 7619,
        token: "tok".into(),
    };
    write(&info, &path).unwrap();
    let back = read(&path).unwrap();
    assert_eq!(back.pid, 12345);
    assert_eq!(back.token, "tok");
}

#[test]
fn generate_token_length() {
    let t = generate_token();
    assert_eq!(t.len(), 64);
    assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn is_alive_current_process() {
    assert!(is_alive(std::process::id()));
}

#[test]
fn is_alive_bogus_pid() {
    assert!(!is_alive(2_000_000));
}

#[test]
fn session_path_contains_pid() {
    let path = session_path(99999);
    assert!(path.to_string_lossy().ends_with("99999.json"));
}

#[test]
fn generate_token_is_unique() {
    assert_ne!(generate_token(), generate_token());
}

#[test]
fn write_overwrites_existing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("99.json");
    let info1 = SessionInfo {
        pid: 99,
        host: "127.0.0.1".into(),
        port: 1234,
        token: "old".into(),
    };
    write(&info1, &path).unwrap();
    let info2 = SessionInfo {
        pid: 99,
        host: "127.0.0.1".into(),
        port: 1234,
        token: "new".into(),
    };
    write(&info2, &path).unwrap();
    let back = read(&path).unwrap();
    assert_eq!(back.token, "new");
}
