use super::super::buffer::TextBuffer;
use crate::action::Motion;
use crate::wrap::OperatorContext;

pub fn place_cursor(buffer: &mut TextBuffer, pos: usize) -> bool {
    buffer.set_cursor(pos).is_ok()
}

pub fn move_cursor(buffer: &mut TextBuffer, motion: Motion) -> bool {
    let before = buffer.cursor();
    super::cursor::move_buffer(buffer, motion, None, OperatorContext::Move, 0, None);
    buffer.cursor() != before
}
