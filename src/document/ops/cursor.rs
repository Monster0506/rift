use super::super::buffer::TextBuffer;
use super::super::Document;
use crate::action::Motion;
use crate::character::Character;
use crate::error::RiftError;
use crate::search::{find_next, SearchDirection};
use crate::wrap::{DisplayMap, OperatorContext};

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
        self.record_cursor_change();
        if matches!(cause, CursorCause::Jump | CursorCause::Lsp) {
            self.buffer.clear_desired_col();
        }
        Ok(())
    }

    pub(crate) fn apply_motion(
        &mut self,
        motion: Motion,
        display_map: Option<&mut DisplayMap>,
        op_ctx: OperatorContext,
        viewport_height: usize,
        last_search_query: Option<&str>,
    ) -> bool {
        let before = self.buffer.cursor();
        move_buffer(
            &mut self.buffer,
            motion,
            display_map,
            op_ctx,
            viewport_height,
            last_search_query,
        );
        let moved = self.buffer.cursor() != before;
        if moved {
            self.record_cursor_change();
        }
        moved
    }

    pub(crate) fn move_cursor(&mut self, motion: Motion) -> bool {
        self.apply_motion(motion, None, OperatorContext::Move, 0, None)
    }

    pub(crate) fn take_char_edits(&mut self) -> Vec<super::super::buffer::CharEdit> {
        self.buffer.take_char_edits()
    }
}

pub(in crate::document) fn move_buffer(
    buf: &mut TextBuffer,
    motion: Motion,
    display_map: Option<&mut DisplayMap>,
    op_ctx: OperatorContext,
    viewport_height: usize,
    last_search_query: Option<&str>,
) {
    match motion {
        Motion::Left => {
            buf.move_left();
        }
        Motion::Right => {
            buf.move_right();
        }
        Motion::Up => {
            if op_ctx == OperatorContext::Move {
                if let Some(dm) = display_map {
                    let col = buf.latch_desired_col(dm.char_to_visual_col_ext(buf.cursor(), buf));
                    let new_pos = dm.visual_up_to_col_ext(buf.cursor(), col, buf);
                    let _ = buf.set_cursor(new_pos);
                    return;
                }
            }
            buf.move_up();
        }
        Motion::Down => {
            if op_ctx == OperatorContext::Move {
                if let Some(dm) = display_map {
                    let col = buf.latch_desired_col(dm.char_to_visual_col_ext(buf.cursor(), buf));
                    let new_pos = dm.visual_down_to_col_ext(buf.cursor(), col, buf);
                    let _ = buf.set_cursor(new_pos);
                    return;
                }
            }
            buf.move_down();
        }
        Motion::StartOfLine => buf.move_to_line_start(),
        Motion::EndOfLine => buf.move_to_line_end(),
        Motion::StartOfFile => buf.move_to_start(),
        Motion::EndOfFile => buf.move_to_end(),
        Motion::PageUp => {
            for _ in 0..viewport_height {
                buf.move_up();
            }
        }
        Motion::PageDown => {
            for _ in 0..viewport_height {
                buf.move_down();
            }
        }
        Motion::NextWord => {
            buf.move_word_right();
        }
        Motion::PreviousWord => {
            buf.move_word_left();
        }
        Motion::WordEnd => {
            buf.move_word_end();
        }
        Motion::NextBigWord => {
            buf.move_big_word_right();
        }
        Motion::PreviousBigWord => {
            buf.move_big_word_left();
        }
        Motion::NextParagraph => {
            buf.move_paragraph_forward();
        }
        Motion::PreviousParagraph => {
            buf.move_paragraph_backward();
        }
        Motion::NextSentence => {
            buf.move_sentence_forward();
        }
        Motion::PreviousSentence => {
            buf.move_sentence_backward();
        }
        Motion::NextMatch => jump_to_match(buf, last_search_query, SearchDirection::Forward),
        Motion::PreviousMatch => {
            jump_to_match(buf, last_search_query, SearchDirection::Backward);
        }
        Motion::TillCharForward(ch) => till_char_forward(buf, ch, op_ctx),
        Motion::TillCharBackward(ch) => till_char_backward(buf, ch),
        Motion::RepeatFindForward | Motion::RepeatFindBackward | Motion::TextObject(_) => {}
        Motion::ToLine(line_idx) => {
            buf.clear_desired_col();
            let start = buf.line_index.get_start(line_idx).unwrap_or(0);
            let _ = buf.set_cursor(start);
        }
        Motion::FindCharForward(ch) => find_char_forward(buf, ch),
        Motion::FindCharBackward(ch) => find_char_backward(buf, ch),
    }
}

fn till_char_forward(buf: &mut TextBuffer, ch: char, op_ctx: OperatorContext) {
    buf.clear_desired_col();
    let target = Character::from(ch);
    let current_line = buf.get_line();
    let total_len = buf.len();
    let line_end = buf
        .line_index
        .get_end(current_line, total_len)
        .unwrap_or(total_len);
    let cursor = buf.cursor();
    let start = if buf.char_at(cursor + 1) == Some(target) {
        cursor + 2
    } else {
        cursor + 1
    };
    for pos in start..line_end {
        if buf.char_at(pos) == Some(target) {
            let land = if op_ctx == OperatorContext::Operator {
                pos
            } else {
                pos.saturating_sub(1)
            };
            let _ = buf.set_cursor(land);
            return;
        }
    }
}

fn till_char_backward(buf: &mut TextBuffer, ch: char) {
    buf.clear_desired_col();
    let target = Character::from(ch);
    let current_line = buf.get_line();
    let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
    let cursor = buf.cursor();
    if cursor > line_start {
        let end = if cursor > 0 && buf.char_at(cursor - 1) == Some(target) {
            cursor - 1
        } else {
            cursor
        };
        if end > line_start {
            for pos in (line_start..end).rev() {
                if buf.char_at(pos) == Some(target) {
                    let _ = buf.set_cursor(pos + 1);
                    return;
                }
            }
        }
    }
}

fn find_char_forward(buf: &mut TextBuffer, ch: char) {
    buf.clear_desired_col();
    let target = Character::from(ch);
    let current_line = buf.get_line();
    let total_len = buf.len();
    let line_end = buf
        .line_index
        .get_end(current_line, total_len)
        .unwrap_or(total_len);
    let start = buf.cursor() + 1;
    for pos in start..line_end {
        if buf.char_at(pos) == Some(target) {
            let _ = buf.set_cursor(pos);
            return;
        }
    }
}

fn find_char_backward(buf: &mut TextBuffer, ch: char) {
    buf.clear_desired_col();
    let target = Character::from(ch);
    let current_line = buf.get_line();
    let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
    let cursor = buf.cursor();
    if cursor > line_start {
        for pos in (line_start..cursor).rev() {
            if buf.char_at(pos) == Some(target) {
                let _ = buf.set_cursor(pos);
                return;
            }
        }
    }
}

fn jump_to_match(
    buf: &mut TextBuffer,
    last_search_query: Option<&str>,
    direction: SearchDirection,
) {
    buf.clear_desired_col();
    let Some(query) = last_search_query else {
        return;
    };
    let start = match direction {
        SearchDirection::Forward => buf.cursor().saturating_add(1),
        SearchDirection::Backward => buf.cursor(),
    };
    if let Ok((Some(m), _stats)) = find_next(buf, start, query, direction) {
        let _ = buf.set_cursor(m.range.start);
    }
}
