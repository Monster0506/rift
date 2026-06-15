use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender};
use std::time::Duration;

use crate::key::Key;
use crate::term::{CursorShape, Size, TerminalBackend};

pub struct ChannelBackend {
    input_rx: Receiver<Key>,
    output_tx: Sender<Vec<u8>>,
    detach_tx: Option<SyncSender<()>>,
    size: Size,
    render_buf: Vec<u8>,
    pending_key: Option<Key>,
}

impl ChannelBackend {
    pub fn new(
        input_rx: Receiver<Key>,
        output_tx: Sender<Vec<u8>>,
        detach_tx: SyncSender<()>,
        size: Size,
    ) -> Self {
        Self {
            input_rx,
            output_tx,
            detach_tx: Some(detach_tx),
            size,
            render_buf: Vec::new(),
            pending_key: None,
        }
    }
}

impl TerminalBackend for ChannelBackend {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn deinit(&mut self) {}

    fn poll(&mut self, duration: Duration) -> Result<bool, String> {
        match self.input_rx.recv_timeout(duration) {
            Ok(Key::Resize(cols, rows)) => {
                self.size = Size { rows, cols };
                self.pending_key = Some(Key::Resize(cols, rows));
                Ok(true)
            }
            Ok(key) => {
                self.pending_key = Some(key);
                Ok(true)
            }
            Err(RecvTimeoutError::Timeout) => Ok(false),
            Err(RecvTimeoutError::Disconnected) => Err("input channel disconnected".into()),
        }
    }

    fn read_key(&mut self) -> Result<Option<Key>, String> {
        Ok(self.pending_key.take())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.render_buf.extend_from_slice(bytes);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), String> {
        if self.render_buf.is_empty() {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.render_buf);
        self.output_tx
            .send(bytes)
            .map_err(|_| "output channel disconnected".into())
    }

    fn get_size(&self) -> Result<Size, String> {
        Ok(self.size)
    }

    fn clear_screen(&mut self) -> Result<(), String> {
        self.render_buf.extend_from_slice(b"\x1b[2J");
        Ok(())
    }

    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), String> {
        use std::io::Write;
        write!(self.render_buf, "\x1b[{};{}H", row + 1, col + 1).map_err(|e| e.to_string())
    }

    fn hide_cursor(&mut self) -> Result<(), String> {
        self.render_buf.extend_from_slice(b"\x1b[?25l");
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), String> {
        self.render_buf.extend_from_slice(b"\x1b[?25h");
        Ok(())
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        self.render_buf.extend_from_slice(b"\x1b[K");
        Ok(())
    }

    fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), String> {
        let seq = match shape {
            CursorShape::SteadyBlock => b"\x1b[2 q" as &[u8],
            CursorShape::SteadyBar => b"\x1b[6 q",
        };
        self.render_buf.extend_from_slice(seq);
        Ok(())
    }

    fn request_detach(&mut self) {
        if let Some(tx) = &self.detach_tx {
            let _ = tx.try_send(());
        }
    }
}

#[cfg(test)]
mod tests {
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
}
