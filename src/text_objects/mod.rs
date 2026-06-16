use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::wrap::{MotionRange, RangeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Inner,
    Around,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Word,
    BigWord,
    DoubleQuote,
    SingleQuote,
    Backtick,
    Paren,
    CurlyBrace,
    SquareBracket,
    AngleBracket,
    Paragraph,
    Sentence,
    Line,
    Buffer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextObjectSpec {
    pub modifier: Modifier,
    pub kind: ObjectKind,
}

pub fn resolve(spec: TextObjectSpec, buf: &TextBuffer) -> Option<MotionRange> {
    let cursor = buf.cursor();
    match spec.kind {
        ObjectKind::Word => resolve_word(cursor, spec.modifier, buf, false),
        ObjectKind::BigWord => resolve_word(cursor, spec.modifier, buf, true),
        ObjectKind::DoubleQuote => resolve_quote(cursor, spec.modifier, buf, '"'),
        ObjectKind::SingleQuote => resolve_quote(cursor, spec.modifier, buf, '\''),
        ObjectKind::Backtick => resolve_quote(cursor, spec.modifier, buf, '`'),
        ObjectKind::Paren => resolve_bracket(cursor, spec.modifier, buf, '(', ')'),
        ObjectKind::CurlyBrace => resolve_bracket(cursor, spec.modifier, buf, '{', '}'),
        ObjectKind::SquareBracket => resolve_bracket(cursor, spec.modifier, buf, '[', ']'),
        ObjectKind::AngleBracket => resolve_bracket(cursor, spec.modifier, buf, '<', '>'),
        ObjectKind::Paragraph => resolve_paragraph(cursor, spec.modifier, buf),
        ObjectKind::Sentence => resolve_sentence(cursor, spec.modifier, buf),
        ObjectKind::Line => resolve_line(cursor, spec.modifier, buf),
        ObjectKind::Buffer => resolve_buffer(buf),
    }
}

fn charwise_inclusive(start: usize, end: usize) -> MotionRange {
    MotionRange {
        anchor: start,
        new_cursor: end,
        kind: RangeKind::Charwise,
        inclusive: true,
    }
}

// Returns 0 = word char, 1 = whitespace, 2 = other (punctuation etc.)
fn char_class_small(ch: Character) -> u8 {
    match ch {
        Character::Unicode(c) if c.is_alphanumeric() || c == '_' => 0,
        Character::Newline | Character::Tab => 1,
        Character::Unicode(c) if c.is_whitespace() => 1,
        _ => 2,
    }
}

// Returns 0 = non-whitespace, 1 = whitespace
fn char_class_big(ch: Character) -> u8 {
    match ch {
        Character::Newline | Character::Tab => 1,
        Character::Unicode(c) if c.is_whitespace() => 1,
        _ => 0,
    }
}

fn resolve_word(
    cursor: usize,
    modifier: Modifier,
    buf: &TextBuffer,
    big: bool,
) -> Option<MotionRange> {
    let len = buf.len();
    if len == 0 {
        return None;
    }

    let ch_at = buf.char_at(cursor)?;
    let class_fn: fn(Character) -> u8 = if big {
        char_class_big
    } else {
        char_class_small
    };
    let cursor_class = class_fn(ch_at);

    let mut start = cursor;
    while start > 0 {
        match buf.char_at(start - 1) {
            Some(ch) if class_fn(ch) == cursor_class => start -= 1,
            _ => break,
        }
    }

    let mut end = cursor;
    while end + 1 < len {
        match buf.char_at(end + 1) {
            Some(ch) if class_fn(ch) == cursor_class => end += 1,
            _ => break,
        }
    }

    match modifier {
        Modifier::Inner => Some(charwise_inclusive(start, end)),
        Modifier::Around => {
            // Prefer eating trailing whitespace; fall back to leading.
            let mut aend = end;
            while aend + 1 < len {
                match buf.char_at(aend + 1) {
                    Some(Character::Tab) | Some(Character::Unicode(' ')) => aend += 1,
                    _ => break,
                }
            }
            if aend > end {
                Some(charwise_inclusive(start, aend))
            } else {
                let mut astart = start;
                while astart > 0 {
                    match buf.char_at(astart - 1) {
                        Some(Character::Tab) | Some(Character::Unicode(' ')) => astart -= 1,
                        _ => break,
                    }
                }
                Some(charwise_inclusive(astart, end))
            }
        }
    }
}

fn find_bracket_open(
    cursor: usize,
    buf: &TextBuffer,
    open_ch: Character,
    close_ch: Character,
) -> Option<usize> {
    let at_cursor = buf.char_at(cursor)?;
    if at_cursor == open_ch {
        return Some(cursor);
    }

    let on_close = at_cursor == close_ch;
    let mut depth = 0usize;
    let scan_from = if on_close {
        cursor.checked_sub(1)?
    } else {
        cursor
    };

    let mut pos = scan_from;
    loop {
        match buf.char_at(pos) {
            Some(ch) if ch == close_ch => depth += 1,
            Some(ch) if ch == open_ch => {
                if depth == 0 {
                    return Some(pos);
                }
                depth -= 1;
            }
            _ => {}
        }
        if pos == 0 {
            break;
        }
        pos -= 1;
    }

    // Not enclosed in a bracket and not on a close bracket: scan forward to the
    // next open bracket on the current line (default "next on line" behavior).
    if !on_close {
        let len = buf.len();
        let line = buf.line_index.get_line_at(cursor);
        let line_end = buf.line_index.get_end(line, len).unwrap_or(len);
        let mut fwd = cursor + 1;
        while fwd < line_end {
            if buf.char_at(fwd) == Some(open_ch) {
                return Some(fwd);
            }
            fwd += 1;
        }
    }

    None
}

fn find_bracket_close(
    open_pos: usize,
    buf: &TextBuffer,
    open_ch: Character,
    close_ch: Character,
) -> Option<usize> {
    let len = buf.len();
    let mut depth = 0usize;
    let mut pos = open_pos + 1;
    while pos < len {
        match buf.char_at(pos) {
            Some(ch) if ch == open_ch => depth += 1,
            Some(ch) if ch == close_ch => {
                if depth == 0 {
                    return Some(pos);
                }
                depth -= 1;
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

fn resolve_bracket(
    cursor: usize,
    modifier: Modifier,
    buf: &TextBuffer,
    open: char,
    close: char,
) -> Option<MotionRange> {
    let open_ch = Character::from(open);
    let close_ch = Character::from(close);

    let open_pos = find_bracket_open(cursor, buf, open_ch, close_ch)?;
    let close_pos = find_bracket_close(open_pos, buf, open_ch, close_ch)?;

    match modifier {
        Modifier::Inner => {
            if open_pos + 1 >= close_pos {
                return None;
            }
            Some(charwise_inclusive(open_pos + 1, close_pos - 1))
        }
        Modifier::Around => Some(charwise_inclusive(open_pos, close_pos)),
    }
}

fn resolve_quote(
    cursor: usize,
    modifier: Modifier,
    buf: &TextBuffer,
    q: char,
) -> Option<MotionRange> {
    let quote = Character::from(q);
    let len = buf.len();
    let current_line = buf.line_index.get_line_at(cursor);
    let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
    let line_end = buf.line_index.get_end(current_line, len).unwrap_or(len);

    let is_escaped =
        |pos: usize| -> bool { pos > 0 && buf.char_at(pos - 1) == Some(Character::Unicode('\\')) };

    // Scan backward on the current line for the opening quote.
    let open_pos = {
        let mut pos = cursor;
        let mut found = None;
        loop {
            if buf.char_at(pos) == Some(quote) && !is_escaped(pos) {
                found = Some(pos);
                break;
            }
            if pos == line_start {
                break;
            }
            pos -= 1;
        }
        found?
    };

    // Scan forward for the closing quote.
    let close_pos = {
        let mut pos = open_pos + 1;
        let mut found = None;
        while pos < line_end {
            if buf.char_at(pos) == Some(quote) && !is_escaped(pos) {
                found = Some(pos);
                break;
            }
            pos += 1;
        }
        found?
    };

    match modifier {
        Modifier::Inner => {
            if open_pos + 1 >= close_pos {
                return None;
            }
            Some(charwise_inclusive(open_pos + 1, close_pos - 1))
        }
        Modifier::Around => Some(charwise_inclusive(open_pos, close_pos)),
    }
}

fn is_blank_line(line: usize, buf: &TextBuffer) -> bool {
    let len = buf.len();
    let start = buf.line_index.get_start(line).unwrap_or(0);
    let end = buf.line_index.get_end(line, len).unwrap_or(len);
    for pos in start..end {
        match buf.char_at(pos) {
            Some(Character::Unicode(c)) if !c.is_whitespace() => return false,
            None => return true,
            _ => {}
        }
    }
    true
}

fn resolve_paragraph(cursor: usize, modifier: Modifier, buf: &TextBuffer) -> Option<MotionRange> {
    let len = buf.len();
    if len == 0 {
        return None;
    }
    let total_lines = buf.get_total_lines();
    let current_line = buf.line_index.get_line_at(cursor);
    let cur_is_blank = is_blank_line(current_line, buf);

    // Expand to cover all contiguous lines of the same kind.
    let mut start_line = current_line;
    while start_line > 0 && is_blank_line(start_line - 1, buf) == cur_is_blank {
        start_line -= 1;
    }

    let mut end_line = current_line;
    while end_line + 1 < total_lines && is_blank_line(end_line + 1, buf) == cur_is_blank {
        end_line += 1;
    }

    let start = buf.line_index.get_start(start_line).unwrap_or(0);
    // Include the newline of end_line by using start of the following line.
    let end = if end_line + 1 < total_lines {
        buf.line_index
            .get_start(end_line + 1)
            .unwrap_or(len)
            .saturating_sub(1)
    } else {
        len.saturating_sub(1)
    };

    match modifier {
        Modifier::Inner => Some(MotionRange {
            anchor: start,
            new_cursor: end,
            kind: RangeKind::Linewise,
            inclusive: true,
        }),
        Modifier::Around => {
            // For non-blank paragraph: also eat the following blank lines.
            // For blank run: also eat the preceding non-blank paragraph.
            if !cur_is_blank {
                let mut around_end_line = end_line;
                while around_end_line + 1 < total_lines && is_blank_line(around_end_line + 1, buf) {
                    around_end_line += 1;
                }
                let aend = if around_end_line + 1 < total_lines {
                    buf.line_index
                        .get_start(around_end_line + 1)
                        .unwrap_or(len)
                        .saturating_sub(1)
                } else {
                    len.saturating_sub(1)
                };
                Some(MotionRange {
                    anchor: start,
                    new_cursor: aend,
                    kind: RangeKind::Linewise,
                    inclusive: true,
                })
            } else {
                // On blank: eat the preceding paragraph.
                let mut around_start_line = start_line;
                if start_line > 0 {
                    let prev = start_line - 1;
                    if !is_blank_line(prev, buf) {
                        around_start_line = prev;
                        while around_start_line > 0 && !is_blank_line(around_start_line - 1, buf) {
                            around_start_line -= 1;
                        }
                    }
                }
                let astart = buf.line_index.get_start(around_start_line).unwrap_or(0);
                Some(MotionRange {
                    anchor: astart,
                    new_cursor: end,
                    kind: RangeKind::Linewise,
                    inclusive: true,
                })
            }
        }
    }
}

fn resolve_sentence(cursor: usize, modifier: Modifier, buf: &TextBuffer) -> Option<MotionRange> {
    let len = buf.len();
    if len == 0 {
        return None;
    }

    let is_sentence_end = |ch: Character| matches!(ch, Character::Unicode('.' | '!' | '?'));

    // Find sentence start: scan backward to previous terminator + trailing space.
    let mut start = cursor;
    let mut prev_end_pos: Option<usize> = None;
    let mut pos = cursor;
    loop {
        match buf.char_at(pos) {
            Some(ch) if is_sentence_end(ch) => {
                prev_end_pos = Some(pos);
                break;
            }
            Some(Character::Newline) => break,
            _ => {}
        }
        if pos == 0 {
            break;
        }
        pos -= 1;
    }

    if let Some(end_pos) = prev_end_pos {
        // Sentence starts after the terminator + any whitespace.
        start = end_pos + 1;
        while start < len {
            match buf.char_at(start) {
                Some(Character::Unicode(c)) if c.is_whitespace() => start += 1,
                _ => break,
            }
        }
    }

    // Find sentence end: scan forward to next terminator.
    let mut end = cursor;
    while end < len {
        if is_sentence_end(buf.char_at(end).unwrap_or(Character::Newline)) {
            break;
        }
        if matches!(buf.char_at(end), Some(Character::Newline)) {
            break;
        }
        end += 1;
    }

    if start > end {
        return None;
    }

    match modifier {
        Modifier::Inner => Some(charwise_inclusive(start, end.saturating_sub(1).max(start))),
        Modifier::Around => {
            // Include the terminator and trailing whitespace.
            let mut aend = end;
            while aend < len {
                match buf.char_at(aend) {
                    Some(Character::Unicode(c)) if c.is_whitespace() => aend += 1,
                    _ => break,
                }
            }
            Some(charwise_inclusive(start, aend.saturating_sub(1).max(start)))
        }
    }
}

fn resolve_line(cursor: usize, modifier: Modifier, buf: &TextBuffer) -> Option<MotionRange> {
    let len = buf.len();
    let current_line = buf.line_index.get_line_at(cursor);
    let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
    // get_end points to the '\n' for non-last lines, or total_len for the last.
    let line_end = buf.line_index.get_end(current_line, len).unwrap_or(len);

    if line_start >= line_end {
        return None;
    }

    match modifier {
        Modifier::Inner => {
            // Content without the newline character.
            Some(charwise_inclusive(line_start, line_end.saturating_sub(1)))
        }
        Modifier::Around => {
            // Content including the newline (or the last char if no newline).
            Some(charwise_inclusive(line_start, line_end))
        }
    }
}

fn resolve_buffer(buf: &TextBuffer) -> Option<MotionRange> {
    let len = buf.len();
    if len == 0 {
        return None;
    }
    Some(charwise_inclusive(0, len - 1))
}

/// For `ci<delim>` on an empty delimiter pair (e.g. cursor inside `()`):
/// returns the byte position to place the insert cursor, or `None` if no
/// matching delimiter pair was found at or ahead of the cursor.
///
/// Only meaningful when `resolve` returned `None` for `Modifier::Inner`.
pub fn resolve_insert_cursor(spec: TextObjectSpec, buf: &TextBuffer) -> Option<usize> {
    if spec.modifier != Modifier::Inner {
        return None;
    }
    let cursor = buf.cursor();
    match spec.kind {
        ObjectKind::Paren => {
            Some(find_bracket_open(cursor, buf, Character::from('('), Character::from(')'))? + 1)
        }
        ObjectKind::CurlyBrace => {
            Some(find_bracket_open(cursor, buf, Character::from('{'), Character::from('}'))? + 1)
        }
        ObjectKind::SquareBracket => {
            Some(find_bracket_open(cursor, buf, Character::from('['), Character::from(']'))? + 1)
        }
        ObjectKind::AngleBracket => {
            Some(find_bracket_open(cursor, buf, Character::from('<'), Character::from('>'))? + 1)
        }
        ObjectKind::DoubleQuote => quote_insert_cursor(cursor, buf, '"'),
        ObjectKind::SingleQuote => quote_insert_cursor(cursor, buf, '\''),
        ObjectKind::Backtick => quote_insert_cursor(cursor, buf, '`'),
        _ => None,
    }
}

fn quote_insert_cursor(cursor: usize, buf: &TextBuffer, q: char) -> Option<usize> {
    let quote = Character::from(q);
    let current_line = buf.line_index.get_line_at(cursor);
    let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
    let is_escaped = |p: usize| p > 0 && buf.char_at(p - 1) == Some(Character::Unicode('\\'));
    let mut pos = cursor;
    loop {
        if buf.char_at(pos) == Some(quote) && !is_escaped(pos) {
            return Some(pos + 1);
        }
        if pos == line_start {
            return None;
        }
        pos -= 1;
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
