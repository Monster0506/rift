use super::super::Document;
use crate::error::RiftError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorCause {
    User,
    Jump,
    Plugin,
    Lsp,
}

impl Document {
    pub(crate) fn set_cursor(&mut self, pos: usize, cause: CursorCause) -> Result<(), RiftError> {
        self.buffer.set_cursor(pos)?;
        if matches!(cause, CursorCause::Jump | CursorCause::Lsp) {
            self.buffer.clear_desired_col();
        }
        Ok(())
    }
}
