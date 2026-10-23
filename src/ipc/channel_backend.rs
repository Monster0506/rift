use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender};
use std::time::Duration;

use crate::error::RiftError;
use crate::key::InputEvent;
use crate::term::{CursorShape, Size, TerminalBackend};

pub(super) struct ChannelBackend {
    input_rx: Receiver<InputEvent>,
    output_tx: Sender<Vec<u8>>,
    detach_tx: Option<SyncSender<()>>,
    size: Size,
    render_buf: Vec<u8>,
    pending_event: Option<InputEvent>,
}

impl ChannelBackend {
    #[must_use]
    pub(super) const fn new(
        input_rx: Receiver<InputEvent>,
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
            pending_event: None,
        }
    }
}

impl TerminalBackend for ChannelBackend {
    fn init(&mut self) -> Result<(), RiftError> {
        Ok(())
    }

    fn deinit(&mut self) {}

    fn poll(&mut self, duration: Duration) -> Result<bool, RiftError> {
        match self.input_rx.recv_timeout(duration) {
            Ok(InputEvent::Resize(cols, rows)) => {
                self.size = Size { rows, cols };
                self.pending_event = Some(InputEvent::Resize(cols, rows));
                Ok(true)
            }
            Ok(event) => {
                self.pending_event = Some(event);
                Ok(true)
            }
            Err(RecvTimeoutError::Timeout) => Ok(false),
            Err(RecvTimeoutError::Disconnected) => Err("input channel disconnected".into()),
        }
    }

    fn read_event(&mut self) -> Result<Option<InputEvent>, RiftError> {
        Ok(self.pending_event.take())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), RiftError> {
        self.render_buf.extend_from_slice(bytes);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RiftError> {
        if self.render_buf.is_empty() {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.render_buf);
        self.output_tx
            .send(bytes)
            .map_err(|e| format!("output channel disconnected: {e}").into())
    }

    fn get_size(&self) -> Result<Size, RiftError> {
        Ok(self.size)
    }

    fn clear_screen(&mut self) -> Result<(), RiftError> {
        self.render_buf.extend_from_slice(b"\x1b[2J");
        Ok(())
    }

    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), RiftError> {
        use std::io::Write;
        write!(self.render_buf, "\x1b[{};{}H", row + 1, col + 1).map_err(|e| e.to_string().into())
    }

    fn hide_cursor(&mut self) -> Result<(), RiftError> {
        self.render_buf.extend_from_slice(b"\x1b[?25l");
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), RiftError> {
        self.render_buf.extend_from_slice(b"\x1b[?25h");
        Ok(())
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), RiftError> {
        self.render_buf.extend_from_slice(b"\x1b[K");
        Ok(())
    }

    fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), RiftError> {
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
#[path = "channel_backend_tests.rs"]
mod channel_backend_tests;
