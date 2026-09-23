use super::*;

#[test]
fn read_one_frame_rejects_oversized_content_length_without_allocating() {
    // The daemon side of the handshake is untrusted at this point (no
    // auth yet); a huge claimed length must not reach `vec![0u8; n]`.
    let mut data: &[u8] = b"Content-Length: 999999999999\r\n\r\n";
    let err = read_one_frame(&mut data).unwrap_err();
    assert!(err.to_string().contains("exceeds max frame size"));
}
