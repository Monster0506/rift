use super::*;
use std::sync::mpsc::{channel, sync_channel};

fn make_backend(
    rows: u16,
    cols: u16,
) -> (
    ChannelBackend,
    std::sync::mpsc::SyncSender<Key>,
    std::sync::mpsc::Receiver<Vec<u8>>,
    std::sync::mpsc::Receiver<()>,
) {
    let (input_tx, input_rx) = sync_channel::<Key>(8);
    let (output_tx, output_rx) = channel::<Vec<u8>>();
    let (detach_tx, detach_rx) = sync_channel::<()>(1);
    let backend = ChannelBackend::new(input_rx, output_tx, detach_tx, Size { rows, cols });
    (backend, input_tx, output_rx, detach_rx)
}

#[test]
fn get_size_returns_initial_size() {
    let (backend, _, _, _) = make_backend(24, 80);
    let size = backend.get_size().unwrap();
    assert_eq!(size.rows, 24);
    assert_eq!(size.cols, 80);
}

#[test]
fn poll_returns_true_on_key() {
    let (mut backend, input_tx, _, _) = make_backend(24, 80);
    input_tx.send(Key::Char('x')).unwrap();
    assert!(backend.poll(Duration::from_millis(100)).unwrap());
}

#[test]
fn poll_returns_false_on_timeout() {
    let (mut backend, _input_tx, _, _) = make_backend(24, 80);
    assert!(!backend.poll(Duration::from_millis(1)).unwrap());
}

#[test]
fn poll_updates_size_on_resize() {
    let (mut backend, input_tx, _, _) = make_backend(24, 80);
    input_tx.send(Key::Resize(100, 50)).unwrap();
    backend.poll(Duration::from_millis(100)).unwrap();
    let size = backend.get_size().unwrap();
    assert_eq!(size.cols, 100);
    assert_eq!(size.rows, 50);
}

#[test]
fn read_key_returns_pending_and_clears() {
    let (mut backend, input_tx, _, _) = make_backend(24, 80);
    input_tx.send(Key::Char('a')).unwrap();
    backend.poll(Duration::from_millis(100)).unwrap();
    assert_eq!(backend.read_key().unwrap(), Some(Key::Char('a')));
    assert_eq!(backend.read_key().unwrap(), None);
}

#[test]
fn write_and_flush_sends_frame() {
    let (mut backend, _, output_rx, _) = make_backend(24, 80);
    backend.write(b"hello").unwrap();
    backend.write(b" world").unwrap();
    backend.flush().unwrap();
    let frame = output_rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert_eq!(frame, b"hello world");
}

#[test]
fn flush_noop_when_empty() {
    let (mut backend, _, output_rx, _) = make_backend(24, 80);
    backend.flush().unwrap();
    assert!(output_rx.try_recv().is_err());
}

#[test]
fn flush_returns_err_when_output_disconnected() {
    let (mut backend, _, output_rx, _) = make_backend(24, 80);
    drop(output_rx);
    backend.write(b"data").unwrap();
    assert!(backend.flush().is_err());
}

#[test]
fn poll_returns_err_when_input_disconnected() {
    let (mut backend, input_tx, _, _) = make_backend(24, 80);
    drop(input_tx);
    assert!(backend.poll(Duration::from_millis(10)).is_err());
}

#[test]
fn request_detach_sends_on_channel() {
    let (mut backend, _, _, detach_rx) = make_backend(24, 80);
    backend.request_detach();
    assert!(detach_rx.try_recv().is_ok());
}
