use super::*;
use crate::error::RiftError;
use crate::term::{CursorShape, Size};
use crate::test_utils::MockTerminal;
use std::collections::VecDeque;
use std::io::BufReader;
use std::net::TcpListener;

struct ScriptedTerminal {
    inner: MockTerminal,
    events: VecDeque<InputEvent>,
}

impl TerminalBackend for ScriptedTerminal {
    fn init(&mut self) -> Result<(), RiftError> {
        self.inner.init()
    }
    fn deinit(&mut self) {
        self.inner.deinit();
    }
    fn poll(&mut self, _duration: Duration) -> Result<bool, RiftError> {
        Ok(!self.events.is_empty())
    }
    fn read_event(&mut self) -> Result<Option<InputEvent>, RiftError> {
        Ok(self.events.pop_front())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), RiftError> {
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> Result<(), RiftError> {
        self.inner.flush()
    }
    fn get_size(&self) -> Result<Size, RiftError> {
        self.inner.get_size()
    }
    fn clear_screen(&mut self) -> Result<(), RiftError> {
        self.inner.clear_screen()
    }
    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), RiftError> {
        self.inner.move_cursor(row, col)
    }
    fn hide_cursor(&mut self) -> Result<(), RiftError> {
        self.inner.hide_cursor()
    }
    fn show_cursor(&mut self) -> Result<(), RiftError> {
        self.inner.show_cursor()
    }
    fn clear_to_end_of_line(&mut self) -> Result<(), RiftError> {
        self.inner.clear_to_end_of_line()
    }
    fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), RiftError> {
        self.inner.set_cursor_shape(shape)
    }
}

#[test]
fn paste_is_sent_as_one_message() {
    const TEXT: &str = "a\n\tb";
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    let mut term = ScriptedTerminal {
        inner: MockTerminal::new(24, 80),
        events: VecDeque::from([InputEvent::Paste(TEXT.into())]),
    };
    let client_loop = std::thread::spawn(move || event_loop(&mut term, client));

    let first = read_framed(&mut BufReader::new(&server)).unwrap();
    let end =
        serde_json::json!({"jsonrpc":"2.0","method":"session.ending","params":{"reason":"test"}});
    write_framed(&mut server, &end).unwrap();
    client_loop.join().unwrap().unwrap();

    let msg: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(msg["method"], "input.paste");
    assert_eq!(msg["params"]["text"], TEXT);
}

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
