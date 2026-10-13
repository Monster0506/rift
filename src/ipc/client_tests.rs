use super::*;

#[test]
fn read_one_frame_rejects_oversized_content_length_without_allocating() {
    // The daemon side of the handshake is untrusted at this point (no
    // auth yet); a huge claimed length must not reach `vec![0u8; n]`.
    let mut data: &[u8] = b"Content-Length: 999999999999\r\n\r\n";
    let err = read_one_frame(&mut data).unwrap_err();
    assert!(err.to_string().contains("exceeds max frame size"));
}

#[test]
fn check_handshake_response_accepts_a_result_payload() {
    let body = br#"{"jsonrpc":"2.0","id":1,"result":{"session_id":"abc"}}"#;
    assert!(check_handshake_response(body).is_ok());
}

#[test]
fn check_handshake_response_rejects_an_error_payload() {
    let body = br#"{"jsonrpc":"2.0","id":1,"error":{"code":401,"message":"invalid token"}}"#;
    let err = check_handshake_response(body).unwrap_err();
    assert!(err.to_string().contains("invalid token"));
}
