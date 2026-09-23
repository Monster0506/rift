use super::*;
use std::io::Cursor;

#[test]
fn round_trip_framed_message() {
    let msg = serde_json::json!({"method": "ping", "params": {"x": 42}});
    let mut buf: Vec<u8> = Vec::new();
    write_framed(&mut buf, &msg).unwrap();
    let mut reader = std::io::BufReader::new(Cursor::new(buf));
    let body = read_framed(&mut reader).unwrap();
    let got: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(got["params"]["x"], 42);
}

#[test]
fn header_format_is_correct() {
    let msg = serde_json::json!({});
    let mut buf: Vec<u8> = Vec::new();
    write_framed(&mut buf, &msg).unwrap();
    let s = String::from_utf8(buf).unwrap();
    assert!(s.starts_with("Content-Length: 2\r\n\r\n"));
}

#[test]
fn frame_message_matches_what_write_framed_would_write() {
    let msg = serde_json::json!({"method": "ping", "params": {"x": 42}});
    let mut via_write: Vec<u8> = Vec::new();
    write_framed(&mut via_write, &msg).unwrap();
    let via_frame = frame_message(&msg).unwrap();
    assert_eq!(via_frame, via_write);
}

#[test]
fn read_framed_rejects_missing_header() {
    let data = b"no-header\r\n\r\n{}";
    let mut reader = std::io::BufReader::new(Cursor::new(data.as_ref()));
    let err = read_framed(&mut reader).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn read_framed_rejects_oversized_content_length_without_allocating() {
    // A malicious/buggy peer claims a huge body but sends none; this must
    // error out before `vec![0u8; n]` tries to allocate gigabytes.
    let data = b"Content-Length: 999999999999\r\n\r\n";
    let mut reader = std::io::BufReader::new(Cursor::new(data.as_ref()));
    let err = read_framed(&mut reader).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}
