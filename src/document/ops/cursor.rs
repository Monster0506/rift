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
    Programmatic,
    Jump,
    #[cfg(feature = "plugins")]
    Plugin,
    #[cfg(feature = "lsp")]
    Lsp,
}

impl Document {
    pub(crate) fn set_cursor(&mut self, pos: usize, cause: CursorCause) -> Result<(), RiftError> {
        self.buffer.set_cursor(pos)?;
        self.record_cursor_change();
        let clears_desired_col = match cause {
            CursorCause::User | CursorCause::Programmatic => false,
            CursorCause::Jump => true,
            #[cfg(feature = "plugins")]
            CursorCause::Plugin => false,
            #[cfg(feature = "lsp")]
            CursorCause::Lsp => true,
        };
        if clears_desired_col {
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

pub(crate) fn compute_motion_range(
    motion: Motion,
    count: usize,
    doc: &mut Document,
    viewport_height: usize,
    last_search_query: Option<&str>,
) -> Option<crate::wrap::MotionRange> {
    use crate::wrap::{MotionRange, OperatorContext};
    const MAX_MOTION_REPEAT: usize = 1_000_000;

    if let Motion::TextObject(spec) = motion {
        #[cfg(feature = "treesitter")]
        {
            let needs_tree = crate::text_objects::requires_treesitter(spec.kind);
            let ts_bytes: Option<Vec<u8>> =
                if needs_tree && doc.syntax.as_ref().and_then(|s| s.tree.as_ref()).is_some() {
                    Some(doc.buffer().to_logical_bytes())
                } else {
                    None
                };
            let syntax_ctx = ts_bytes.as_ref().and_then(|bytes| {
                doc.syntax
                    .as_ref()
                    .and_then(|s| s.tree.as_ref())
                    .map(|tree| crate::text_objects::treesitter::SyntaxContext {
                        tree,
                        source: bytes.as_slice(),
                    })
            });
            return crate::text_objects::resolve(spec, doc.buffer(), count, syntax_ctx.as_ref());
        }
        #[cfg(not(feature = "treesitter"))]
        {
            return crate::text_objects::resolve(spec, doc.buffer(), count, None);
        }
    }

    let is_jump = matches!(
        motion,
        Motion::ToLine(_) | Motion::StartOfFile | Motion::EndOfFile
    );
    let is_linewise = is_jump
        || matches!(
            motion,
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
        );
    let is_inclusive = matches!(motion, Motion::FindCharForward(_) | Motion::WordEnd);

    let anchor = doc.buffer().cursor();

    let count = count.min(MAX_MOTION_REPEAT);

    for i in 0..count {
        let iter_start = doc.buffer().cursor();

        doc.apply_motion(
            motion,
            None,
            OperatorContext::Operator,
            viewport_height,
            last_search_query,
        );

        if i + 1 == count && matches!(motion, Motion::NextWord | Motion::NextBigWord) {
            clamp_to_line_end(doc, iter_start);
        }
    }

    let new_cursor = doc.buffer().cursor();
    let _ = doc.set_cursor(anchor, crate::document::ops::CursorCause::Programmatic);

    if anchor == new_cursor && !is_jump {
        return None;
    }

    if is_linewise {
        Some(MotionRange::linewise(anchor, new_cursor))
    } else if is_inclusive {
        Some(MotionRange::charwise_inclusive(anchor, new_cursor))
    } else {
        Some(MotionRange::charwise(anchor, new_cursor))
    }
}

pub(super) fn compute_change_range(
    motion: Motion,
    count: usize,
    doc: &mut Document,
    viewport_height: usize,
    last_search_query: Option<&str>,
) -> Option<crate::wrap::MotionRange> {
    if !matches!(motion, Motion::NextWord) || !on_non_blank(doc.buffer()) {
        return compute_motion_range(motion, count, doc, viewport_height, last_search_query);
    }

    let cursor = doc.buffer().cursor();
    let remaining = if on_word_end(doc.buffer()) {
        count.max(1) - 1
    } else {
        count
    };
    if remaining == 0 {
        return Some(crate::wrap::MotionRange::charwise_inclusive(cursor, cursor));
    }

    compute_motion_range(
        Motion::WordEnd,
        remaining,
        doc,
        viewport_height,
        last_search_query,
    )
}

fn on_non_blank(buf: &TextBuffer) -> bool {
    use crate::movement::classify::{classify_character, CharClass};
    buf.char_at(buf.cursor())
        .is_some_and(|c| classify_character(c) != CharClass::Whitespace)
}

fn on_word_end(buf: &TextBuffer) -> bool {
    use crate::movement::classify::classify_character;
    let cursor = buf.cursor();
    let Some(cur) = buf.char_at(cursor) else {
        return false;
    };

    buf.char_at(cursor + 1).map(classify_character) != Some(classify_character(cur))
}

fn clamp_to_line_end(doc: &mut Document, from: usize) {
    let buf = doc.buffer();
    let start_line = buf.line_index.get_line_at(from);
    if buf.line_index.get_line_at(buf.cursor()) <= start_line {
        return;
    }

    let Some(end) = buf.line_index.get_end(start_line, buf.len()) else {
        return;
    };
    if from < end {
        let _ = doc.set_cursor(end, crate::document::ops::CursorCause::Programmatic);
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
