use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::wrap::{MotionRange, RangeKind};

mod treesitter;
pub use treesitter::SyntaxContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Inner,
    Around,
    InnerStrict,
    AroundLoose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Current,
    Next,
    Last,
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
    AnyBracket,
    AnyQuote,
    Paragraph,
    Sentence,
    Line,
    Buffer,
    FunctionCall,
    Argument,
    FunctionDef,
    Class,
    Block,
    Tag,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextObjectSpec {
    pub modifier: Modifier,
    pub direction: Direction,
    pub nesting: u8,
    pub kind: ObjectKind,
}

const BRACKET_PAIRS: [(char, char); 4] = [('(', ')'), ('{', '}'), ('[', ']'), ('<', '>')];
const QUOTE_CHARS: [char; 3] = ['"', '\'', '`'];

/// Object key char -> kind. `n`/`p` are reserved for the direction prefix
/// and never appear here.
pub const OBJECT_KEY_TABLE: &[(char, ObjectKind)] = &[
    ('w', ObjectKind::Word),
    ('W', ObjectKind::BigWord),
    ('"', ObjectKind::DoubleQuote),
    ('\'', ObjectKind::SingleQuote),
    ('`', ObjectKind::Backtick),
    ('(', ObjectKind::Paren),
    (')', ObjectKind::Paren),
    ('B', ObjectKind::CurlyBrace),
    ('[', ObjectKind::SquareBracket),
    (']', ObjectKind::SquareBracket),
    ('<', ObjectKind::AngleBracket),
    ('>', ObjectKind::AngleBracket),
    ('b', ObjectKind::AnyBracket),
    ('q', ObjectKind::AnyQuote),
    ('{', ObjectKind::Paragraph),
    ('}', ObjectKind::Paragraph),
    ('s', ObjectKind::Sentence),
    ('l', ObjectKind::Line),
    ('g', ObjectKind::Buffer),
    ('f', ObjectKind::FunctionCall),
    ('a', ObjectKind::Argument),
    ('F', ObjectKind::FunctionDef),
    ('c', ObjectKind::Class),
    ('o', ObjectKind::Block),
    ('t', ObjectKind::Tag),
    ('d', ObjectKind::Number),
];

/// True for objects that require a tree-sitter parse tree to resolve.
pub fn requires_treesitter(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::FunctionCall
            | ObjectKind::Argument
            | ObjectKind::FunctionDef
            | ObjectKind::Class
            | ObjectKind::Block
            | ObjectKind::Tag
            | ObjectKind::Number
    )
}

pub fn object_kind_for_key(ch: char) -> Option<ObjectKind> {
    OBJECT_KEY_TABLE
        .iter()
        .find(|&&(c, _)| c == ch)
        .map(|&(_, kind)| kind)
}

pub fn modifier_for_key(ch: char) -> Option<Modifier> {
    match ch {
        'i' => Some(Modifier::Inner),
        'a' => Some(Modifier::Around),
        'I' => Some(Modifier::InnerStrict),
        'A' => Some(Modifier::AroundLoose),
        _ => None,
    }
}

/// Composes a leading operator count with an in-grammar nest-count
/// (e.g. `2di2(` => nesting 4), matching vim's general count rule.
fn compose_nesting(spec_nesting: u8, count: usize) -> u8 {
    let n = spec_nesting.max(1) as u32;
    let c = (count.max(1) as u32).min(u8::MAX as u32);
    (n * c).min(u8::MAX as u32) as u8
}

pub fn resolve(
    spec: TextObjectSpec,
    buf: &TextBuffer,
    count: usize,
    syntax: Option<SyntaxContext>,
) -> Option<MotionRange> {
    let cursor = buf.cursor();
    let nesting = compose_nesting(spec.nesting, count);
    let repeat = count.max(1);
    let base_modifier = base_modifier(spec.modifier);

    let resolved = match spec.kind {
        ObjectKind::Word => resolve_word(cursor, base_modifier, buf, false, repeat),
        ObjectKind::BigWord => resolve_word(cursor, base_modifier, buf, true, repeat),
        ObjectKind::DoubleQuote => {
            resolve_quote_nested(cursor, base_modifier, spec.direction, nesting, buf, '"')
        }
        ObjectKind::SingleQuote => {
            resolve_quote_nested(cursor, base_modifier, spec.direction, nesting, buf, '\'')
        }
        ObjectKind::Backtick => {
            resolve_quote_nested(cursor, base_modifier, spec.direction, nesting, buf, '`')
        }
        ObjectKind::Paren => resolve_bracket_nested(
            cursor,
            base_modifier,
            spec.direction,
            nesting,
            buf,
            '(',
            ')',
        ),
        ObjectKind::CurlyBrace => resolve_bracket_nested(
            cursor,
            base_modifier,
            spec.direction,
            nesting,
            buf,
            '{',
            '}',
        ),
        ObjectKind::SquareBracket => resolve_bracket_nested(
            cursor,
            base_modifier,
            spec.direction,
            nesting,
            buf,
            '[',
            ']',
        ),
        ObjectKind::AngleBracket => resolve_bracket_nested(
            cursor,
            base_modifier,
            spec.direction,
            nesting,
            buf,
            '<',
            '>',
        ),
        ObjectKind::AnyBracket => {
            resolve_any_bracket(cursor, base_modifier, spec.direction, nesting, buf)
        }
        ObjectKind::AnyQuote => {
            resolve_any_quote(cursor, base_modifier, spec.direction, nesting, buf)
        }
        ObjectKind::Paragraph => resolve_paragraph(cursor, base_modifier, buf, repeat),
        ObjectKind::Sentence => resolve_sentence(cursor, base_modifier, buf, repeat),
        ObjectKind::Line => resolve_line(cursor, base_modifier, buf, repeat),
        ObjectKind::Buffer => resolve_buffer(buf),
        ObjectKind::FunctionCall
        | ObjectKind::Argument
        | ObjectKind::FunctionDef
        | ObjectKind::Class
        | ObjectKind::Block
        | ObjectKind::Tag
        | ObjectKind::Number => {
            let ctx = syntax.as_ref()?;
            treesitter::resolve(spec.kind, base_modifier, nesting, cursor, buf, ctx)
        }
    }?;

    Some(apply_modifier_post(spec.modifier, resolved, buf))
}

/// The two structural resolution passes; `InnerStrict`/`AroundLoose` run one
/// of these first and then post-process the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaseModifier {
    Inner,
    Around,
}

fn base_modifier(modifier: Modifier) -> BaseModifier {
    match modifier {
        Modifier::Inner | Modifier::InnerStrict => BaseModifier::Inner,
        Modifier::Around | Modifier::AroundLoose => BaseModifier::Around,
    }
}

fn apply_modifier_post(modifier: Modifier, range: MotionRange, buf: &TextBuffer) -> MotionRange {
    match modifier {
        Modifier::Inner | Modifier::Around => range,
        Modifier::InnerStrict => strip_whitespace_ends(range, buf),
        Modifier::AroundLoose => eat_trailing_whitespace(range, buf),
    }
}

fn is_space_or_tab(ch: Character) -> bool {
    matches!(ch, Character::Tab)
        || matches!(ch, Character::Unicode(c) if c.is_whitespace() && c != '\n')
}

fn strip_whitespace_ends(range: MotionRange, buf: &TextBuffer) -> MotionRange {
    if range.kind != RangeKind::Charwise {
        return range;
    }
    let mut start = range.anchor;
    let mut end = range.new_cursor;
    while start <= end {
        match buf.char_at(start) {
            Some(ch) if is_space_or_tab(ch) => start += 1,
            _ => break,
        }
    }
    while end >= start {
        match buf.char_at(end) {
            Some(ch) if is_space_or_tab(ch) => {
                if end == start {
                    break;
                }
                end -= 1;
            }
            _ => break,
        }
    }
    if start > end || (start == end && buf.char_at(start).is_some_and(is_space_or_tab)) {
        return MotionRange {
            anchor: range.anchor,
            new_cursor: range.anchor,
            ..range
        };
    }
    MotionRange {
        anchor: start,
        new_cursor: end,
        ..range
    }
}

fn eat_trailing_whitespace(range: MotionRange, buf: &TextBuffer) -> MotionRange {
    if range.kind != RangeKind::Charwise {
        return range;
    }
    let len = buf.len();
    let mut end = range.new_cursor;
    while end + 1 < len {
        match buf.char_at(end + 1) {
            Some(ch) if is_space_or_tab(ch) => end += 1,
            _ => break,
        }
    }
    MotionRange {
        new_cursor: end,
        ..range
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
    modifier: BaseModifier,
    buf: &TextBuffer,
    big: bool,
    count: usize,
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

    // Extend forward through (count - 1) more word spans, swallowing the
    // whitespace gap (if any) between each, matching vim's counted objects.
    for _ in 1..count.max(1) {
        if end + 1 >= len {
            break;
        }
        let mut next = end + 1;
        while next < len && matches!(buf.char_at(next), Some(ch) if class_fn(ch) == 1) {
            next += 1;
        }
        let Some(next_ch) = buf.char_at(next) else {
            end = len.saturating_sub(1);
            break;
        };
        let next_class = class_fn(next_ch);
        let mut nend = next;
        while nend + 1 < len {
            match buf.char_at(nend + 1) {
                Some(ch) if class_fn(ch) == next_class => nend += 1,
                _ => break,
            }
        }
        end = nend;
    }

    match modifier {
        BaseModifier::Inner => Some(charwise_inclusive(start, end)),
        BaseModifier::Around => {
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

/// Pure backward depth-tracked scan for an enclosing open bracket, starting
/// at (and including) `start_pos`. Used to expand outward for nest-count.
fn find_enclosing_open_strict(
    start_pos: usize,
    buf: &TextBuffer,
    open_ch: Character,
    close_ch: Character,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut pos = start_pos;
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
    None
}

/// Finds the open bracket matching a known close-bracket position, scanning
/// backward with depth tracking.
fn match_open_for_close(
    close_pos: usize,
    buf: &TextBuffer,
    open_ch: Character,
    close_ch: Character,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut pos = close_pos;
    loop {
        if pos == 0 {
            return None;
        }
        pos -= 1;
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
    }
}

fn find_bracket_open_dir(
    cursor: usize,
    buf: &TextBuffer,
    open_ch: Character,
    close_ch: Character,
    direction: Direction,
) -> Option<usize> {
    match direction {
        Direction::Current => find_bracket_open(cursor, buf, open_ch, close_ch),
        Direction::Next => {
            let len = buf.len();
            let mut fwd = cursor + 1;
            while fwd < len {
                if buf.char_at(fwd) == Some(open_ch) {
                    return Some(fwd);
                }
                fwd += 1;
            }
            None
        }
        Direction::Last => {
            // Nearest complete bracket pair entirely before the cursor.
            let mut pos = cursor.checked_sub(1)?;
            loop {
                if buf.char_at(pos) == Some(close_ch) {
                    if let Some(open) = match_open_for_close(pos, buf, open_ch, close_ch) {
                        return Some(open);
                    }
                }
                if pos == 0 {
                    break;
                }
                pos -= 1;
            }
            None
        }
    }
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

/// Walks outward from an already-resolved (open, close) pair to the nesting
/// level requested. `nesting == 1` is a no-op (returns the input unchanged).
fn expand_bracket_nesting(
    open_pos: usize,
    close_pos: usize,
    nesting: u8,
    buf: &TextBuffer,
    open_ch: Character,
    close_ch: Character,
) -> Option<(usize, usize)> {
    let mut open_pos = open_pos;
    let mut close_pos = close_pos;
    for _ in 1..nesting {
        let search_from = open_pos.checked_sub(1)?;
        open_pos = find_enclosing_open_strict(search_from, buf, open_ch, close_ch)?;
        close_pos = find_bracket_close(open_pos, buf, open_ch, close_ch)?;
    }
    Some((open_pos, close_pos))
}

fn bracket_pair_with_nesting(
    cursor: usize,
    direction: Direction,
    nesting: u8,
    buf: &TextBuffer,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let open_ch = Character::from(open);
    let close_ch = Character::from(close);
    let open_pos = find_bracket_open_dir(cursor, buf, open_ch, close_ch, direction)?;
    let close_pos = find_bracket_close(open_pos, buf, open_ch, close_ch)?;
    expand_bracket_nesting(open_pos, close_pos, nesting, buf, open_ch, close_ch)
}

fn bracket_range(modifier: BaseModifier, open_pos: usize, close_pos: usize) -> Option<MotionRange> {
    match modifier {
        BaseModifier::Inner => {
            if open_pos + 1 >= close_pos {
                return None;
            }
            Some(charwise_inclusive(open_pos + 1, close_pos - 1))
        }
        BaseModifier::Around => Some(charwise_inclusive(open_pos, close_pos)),
    }
}

fn resolve_bracket_nested(
    cursor: usize,
    modifier: BaseModifier,
    direction: Direction,
    nesting: u8,
    buf: &TextBuffer,
    open: char,
    close: char,
) -> Option<MotionRange> {
    let (open_pos, close_pos) =
        bracket_pair_with_nesting(cursor, direction, nesting, buf, open, close)?;
    bracket_range(modifier, open_pos, close_pos)
}

/// True if `(open, close)` is a "nearer" match than `(best_open, best_close)`:
/// enclosing pairs win over forward-found, tightest enclosure wins among those.
fn is_closer_pair(
    cursor: usize,
    open: usize,
    close: usize,
    best_open: usize,
    best_close: usize,
) -> bool {
    let _ = (close, best_close);
    let encloses = open <= cursor;
    let best_encloses = best_open <= cursor;
    match (encloses, best_encloses) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => open > best_open,
        (false, false) => open < best_open,
    }
}

fn resolve_any_bracket(
    cursor: usize,
    modifier: BaseModifier,
    direction: Direction,
    nesting: u8,
    buf: &TextBuffer,
) -> Option<MotionRange> {
    let mut best: Option<(usize, usize)> = None;
    for &(open, close) in BRACKET_PAIRS.iter() {
        let Some((open_pos, close_pos)) =
            bracket_pair_with_nesting(cursor, direction, nesting, buf, open, close)
        else {
            continue;
        };
        let better = match best {
            None => true,
            Some((bo, bc)) => is_closer_pair(cursor, open_pos, close_pos, bo, bc),
        };
        if better {
            best = Some((open_pos, close_pos));
        }
    }
    let (open_pos, close_pos) = best?;
    bracket_range(modifier, open_pos, close_pos)
}

/// A quote at `pos` is escaped only when preceded by an odd number of
/// backslashes (an even count escapes each other in pairs, not the quote).
fn is_quote_escaped(buf: &TextBuffer, pos: usize) -> bool {
    let mut count = 0;
    let mut p = pos;
    while p > 0 && buf.char_at(p - 1) == Some(Character::Unicode('\\')) {
        count += 1;
        p -= 1;
    }
    count % 2 == 1
}

fn find_quote_pair_dir(
    cursor: usize,
    buf: &TextBuffer,
    quote: Character,
    direction: Direction,
) -> Option<(usize, usize)> {
    match direction {
        Direction::Current => {
            let len = buf.len();
            let current_line = buf.line_index.get_line_at(cursor);
            let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
            let line_end = buf.line_index.get_end(current_line, len).unwrap_or(len);

            let scan_fwd = |from: usize| -> Option<usize> {
                (from..line_end)
                    .find(|&p| buf.char_at(p) == Some(quote) && !is_quote_escaped(buf, p))
            };
            let scan_back = |from: usize| -> Option<usize> {
                (line_start..=from)
                    .rev()
                    .find(|&p| buf.char_at(p) == Some(quote) && !is_quote_escaped(buf, p))
            };

            if buf.char_at(cursor) == Some(quote) && !is_quote_escaped(buf, cursor) {
                // Cursor is on an unescaped quote: parity from line start
                // says whether it's an opener or closer of its own pair.
                let quotes_through_cursor = (line_start..=cursor)
                    .filter(|&p| buf.char_at(p) == Some(quote) && !is_quote_escaped(buf, p))
                    .count();
                return if quotes_through_cursor % 2 == 1 {
                    scan_fwd(cursor + 1).map(|close| (cursor, close))
                } else {
                    cursor
                        .checked_sub(1)
                        .and_then(scan_back)
                        .map(|open| (open, cursor))
                };
            }

            // Nearest enclosing pair: the closest quote behind the cursor is
            // the opener, paired with the next quote after it.
            if let Some(open_pos) = scan_back(cursor) {
                return scan_fwd(open_pos + 1).map(|close| (open_pos, close));
            }

            // Nothing behind the cursor on this line: fall forward to the
            // next quoted string instead of no-op'ing, matching vim.
            let open_pos = scan_fwd(cursor)?;
            scan_fwd(open_pos + 1).map(|close| (open_pos, close))
        }
        Direction::Next => {
            let len = buf.len();
            let mut pos = cursor + 1;
            let open_pos = loop {
                if pos >= len {
                    return None;
                }
                if buf.char_at(pos) == Some(quote) && !is_quote_escaped(buf, pos) {
                    break pos;
                }
                pos += 1;
            };
            let mut pos2 = open_pos + 1;
            let close_pos = loop {
                if pos2 >= len {
                    return None;
                }
                if buf.char_at(pos2) == Some(quote) && !is_quote_escaped(buf, pos2) {
                    break pos2;
                }
                pos2 += 1;
            };
            Some((open_pos, close_pos))
        }
        Direction::Last => {
            // Nearest complete quote pair entirely before the cursor.
            let mut close_pos = cursor.checked_sub(1)?;
            loop {
                if buf.char_at(close_pos) == Some(quote)
                    && !is_quote_escaped(buf, close_pos)
                    && close_pos > 0
                {
                    let mut p = close_pos - 1;
                    loop {
                        if buf.char_at(p) == Some(quote) && !is_quote_escaped(buf, p) {
                            return Some((p, close_pos));
                        }
                        if p == 0 {
                            break;
                        }
                        p -= 1;
                    }
                }
                if close_pos == 0 {
                    return None;
                }
                close_pos -= 1;
            }
        }
    }
}

/// Walks leftward on the line to the requested nest-count of quote pairs
/// (quotes don't nest, so this picks successively earlier sibling pairs).
fn quote_pair_with_nesting(
    cursor: usize,
    direction: Direction,
    nesting: u8,
    buf: &TextBuffer,
    q: char,
) -> Option<(usize, usize)> {
    let quote = Character::from(q);
    let (mut open_pos, mut close_pos) = find_quote_pair_dir(cursor, buf, quote, direction)?;

    for _ in 1..nesting {
        let current_line = buf.line_index.get_line_at(open_pos);
        let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
        if open_pos == line_start {
            return None;
        }
        let mut p = open_pos - 1;
        let prev_close = loop {
            if buf.char_at(p) == Some(quote) && !is_quote_escaped(buf, p) {
                break p;
            }
            if p == line_start {
                return None;
            }
            p -= 1;
        };
        if prev_close == line_start {
            return None;
        }
        let mut p2 = prev_close - 1;
        let prev_open = loop {
            if buf.char_at(p2) == Some(quote) && !is_quote_escaped(buf, p2) {
                break p2;
            }
            if p2 == line_start {
                return None;
            }
            p2 -= 1;
        };
        open_pos = prev_open;
        close_pos = prev_close;
    }
    Some((open_pos, close_pos))
}

fn resolve_quote_nested(
    cursor: usize,
    modifier: BaseModifier,
    direction: Direction,
    nesting: u8,
    buf: &TextBuffer,
    q: char,
) -> Option<MotionRange> {
    let (open_pos, close_pos) = quote_pair_with_nesting(cursor, direction, nesting, buf, q)?;
    bracket_range(modifier, open_pos, close_pos)
}

fn resolve_any_quote(
    cursor: usize,
    modifier: BaseModifier,
    direction: Direction,
    nesting: u8,
    buf: &TextBuffer,
) -> Option<MotionRange> {
    let mut best: Option<(usize, usize)> = None;
    for &q in QUOTE_CHARS.iter() {
        let Some((open_pos, close_pos)) =
            quote_pair_with_nesting(cursor, direction, nesting, buf, q)
        else {
            continue;
        };
        let better = match best {
            None => true,
            Some((bo, bc)) => is_closer_pair(cursor, open_pos, close_pos, bo, bc),
        };
        if better {
            best = Some((open_pos, close_pos));
        }
    }
    let (open_pos, close_pos) = best?;
    bracket_range(modifier, open_pos, close_pos)
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

fn resolve_paragraph(
    cursor: usize,
    modifier: BaseModifier,
    buf: &TextBuffer,
    count: usize,
) -> Option<MotionRange> {
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

    // Extend through (count - 1) more contiguous groups, alternating kind.
    let mut group_is_blank = cur_is_blank;
    for _ in 1..count.max(1) {
        if end_line + 1 >= total_lines {
            break;
        }
        group_is_blank = !group_is_blank;
        let group_start = end_line + 1;
        let mut g_end = group_start;
        while g_end + 1 < total_lines && is_blank_line(g_end + 1, buf) == group_is_blank {
            g_end += 1;
        }
        end_line = g_end;
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
        BaseModifier::Inner => Some(MotionRange {
            anchor: start,
            new_cursor: end,
            kind: RangeKind::Linewise,
            inclusive: true,
        }),
        BaseModifier::Around => {
            // For non-blank paragraph: also eat the following blank lines.
            // For blank run: also eat the preceding non-blank paragraph.
            if !group_is_blank {
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

fn resolve_sentence(
    cursor: usize,
    modifier: BaseModifier,
    buf: &TextBuffer,
    count: usize,
) -> Option<MotionRange> {
    let len = buf.len();
    if len == 0 {
        return None;
    }

    let is_sentence_end = |ch: Character| matches!(ch, Character::Unicode('.' | '!' | '?'));

    // Scan backward for the previous terminator + trailing space. If the
    // cursor is on a terminator, start one char earlier to find the prior one.
    let on_terminator = matches!(buf.char_at(cursor), Some(ch) if is_sentence_end(ch));
    let mut prev_end_pos: Option<usize> = None;
    let mut newline_boundary: Option<usize> = None;
    if let Some(mut pos) = if on_terminator {
        cursor.checked_sub(1)
    } else {
        Some(cursor)
    } {
        loop {
            match buf.char_at(pos) {
                Some(ch) if is_sentence_end(ch) => {
                    prev_end_pos = Some(pos);
                    break;
                }
                Some(Character::Newline) => {
                    newline_boundary = Some(pos);
                    break;
                }
                _ => {}
            }
            if pos == 0 {
                break;
            }
            pos -= 1;
        }
    }

    // Sentence starts right after the found boundary (or buffer start if
    // none), plus any trailing whitespace.
    let mut start = match (prev_end_pos, newline_boundary) {
        (Some(end_pos), _) => end_pos + 1,
        (None, Some(nl)) => nl + 1,
        (None, None) => 0,
    };
    while start < len {
        match buf.char_at(start) {
            Some(Character::Unicode(c)) if c.is_whitespace() => start += 1,
            _ => break,
        }
    }

    // Find sentence end: scan forward to next terminator, repeated `count`
    // times to extend across that many consecutive sentences.
    let mut end = cursor;
    for step in 0..count.max(1) {
        if step > 0 {
            if end >= len {
                break;
            }
            end += 1;
            while end < len {
                match buf.char_at(end) {
                    Some(Character::Unicode(c)) if c.is_whitespace() => end += 1,
                    _ => break,
                }
            }
            if end >= len {
                break;
            }
        }
        let mut found = false;
        while end < len {
            if is_sentence_end(buf.char_at(end).unwrap_or(Character::Newline)) {
                found = true;
                break;
            }
            if matches!(buf.char_at(end), Some(Character::Newline)) {
                break;
            }
            end += 1;
        }
        if !found {
            break;
        }
    }

    if start > end {
        return None;
    }

    match modifier {
        BaseModifier::Inner => Some(charwise_inclusive(start, end.saturating_sub(1).max(start))),
        BaseModifier::Around => {
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

fn resolve_line(
    cursor: usize,
    modifier: BaseModifier,
    buf: &TextBuffer,
    count: usize,
) -> Option<MotionRange> {
    let len = buf.len();
    let total_lines = buf.get_total_lines();
    let current_line = buf.line_index.get_line_at(cursor);
    let last_line = (current_line + count.max(1) - 1).min(total_lines.saturating_sub(1));
    let line_start = buf.line_index.get_start(current_line).unwrap_or(0);
    // get_end points to the '\n' for non-last lines, or total_len for the last.
    let line_end = buf.line_index.get_end(last_line, len).unwrap_or(len);

    if line_start >= line_end {
        return None;
    }

    match modifier {
        BaseModifier::Inner => {
            // Content without the final line's newline character.
            Some(charwise_inclusive(line_start, line_end.saturating_sub(1)))
        }
        BaseModifier::Around => {
            // Content including the final line's newline (or its last char if none).
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

/// For `ci<delim>` on an empty delimiter pair (e.g. cursor inside `()`),
/// returns the byte position to place the insert cursor, or `None` if none.
pub fn resolve_insert_cursor(
    spec: TextObjectSpec,
    buf: &TextBuffer,
    count: usize,
) -> Option<usize> {
    if base_modifier(spec.modifier) != BaseModifier::Inner {
        return None;
    }
    let cursor = buf.cursor();
    let nesting = compose_nesting(spec.nesting, count);
    match spec.kind {
        ObjectKind::Paren => {
            Some(bracket_pair_with_nesting(cursor, spec.direction, nesting, buf, '(', ')')?.0 + 1)
        }
        ObjectKind::CurlyBrace => {
            Some(bracket_pair_with_nesting(cursor, spec.direction, nesting, buf, '{', '}')?.0 + 1)
        }
        ObjectKind::SquareBracket => {
            Some(bracket_pair_with_nesting(cursor, spec.direction, nesting, buf, '[', ']')?.0 + 1)
        }
        ObjectKind::AngleBracket => {
            Some(bracket_pair_with_nesting(cursor, spec.direction, nesting, buf, '<', '>')?.0 + 1)
        }
        ObjectKind::AnyBracket => {
            Some(any_bracket_insert_pair(cursor, spec.direction, nesting, buf)?.0 + 1)
        }
        ObjectKind::DoubleQuote => {
            Some(quote_pair_with_nesting(cursor, spec.direction, nesting, buf, '"')?.0 + 1)
        }
        ObjectKind::SingleQuote => {
            Some(quote_pair_with_nesting(cursor, spec.direction, nesting, buf, '\'')?.0 + 1)
        }
        ObjectKind::Backtick => {
            Some(quote_pair_with_nesting(cursor, spec.direction, nesting, buf, '`')?.0 + 1)
        }
        ObjectKind::AnyQuote => {
            Some(any_quote_insert_pair(cursor, spec.direction, nesting, buf)?.0 + 1)
        }
        _ => None,
    }
}

fn any_bracket_insert_pair(
    cursor: usize,
    direction: Direction,
    nesting: u8,
    buf: &TextBuffer,
) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for &(open, close) in BRACKET_PAIRS.iter() {
        let Some(pair) = bracket_pair_with_nesting(cursor, direction, nesting, buf, open, close)
        else {
            continue;
        };
        let better = match best {
            None => true,
            Some((bo, bc)) => is_closer_pair(cursor, pair.0, pair.1, bo, bc),
        };
        if better {
            best = Some(pair);
        }
    }
    best
}

fn any_quote_insert_pair(
    cursor: usize,
    direction: Direction,
    nesting: u8,
    buf: &TextBuffer,
) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for &q in QUOTE_CHARS.iter() {
        let Some(pair) = quote_pair_with_nesting(cursor, direction, nesting, buf, q) else {
            continue;
        };
        let better = match best {
            None => true,
            Some((bo, bc)) => is_closer_pair(cursor, pair.0, pair.1, bo, bc),
        };
        if better {
            best = Some(pair);
        }
    }
    best
}

/// Maps a surround key to its `(open, close)` delimiter strings, per
/// design.md's pairing table, each repeated `count` times. Opening bracket
/// chars get an inner space pad; closing chars, letter aliases, and quotes
/// never pad.
pub fn surround_strings(ch: char, count: usize) -> Option<(String, String)> {
    let (open, close) = match ch {
        '(' | ')' | 'b' => ('(', ')'),
        '{' | '}' | 'B' => ('{', '}'),
        '[' | ']' | 'r' => ('[', ']'),
        '<' | '>' => ('<', '>'),
        '"' | '\'' | '`' => (ch, ch),
        _ => return None,
    };
    let count = count.max(1);
    if matches!(ch, '(' | '{' | '[' | '<') {
        Some((
            format!("{open} ").repeat(count),
            format!(" {close}").repeat(count),
        ))
    } else {
        Some((
            open.to_string().repeat(count),
            close.to_string().repeat(count),
        ))
    }
}

/// Locates an existing surround pair enclosing the cursor for `ds`/`cs`,
/// returning the half-open delete ranges for the opening and closing
/// delimiters. `count` expands each boundary outward over up to `count - 1`
/// further consecutive occurrences of the same delimiter char (so `2ds"` on
/// `""text""` removes both quotes on each side), clamping gracefully when
/// fewer repeats are actually present.
pub fn resolve_surround_pair(
    ch: char,
    buf: &TextBuffer,
    count: usize,
) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let cursor = buf.cursor();
    let (open_pos, close_pos, open_ch, close_ch) = match ch {
        '(' | ')' | 'b' => {
            let (o, c) = bracket_pair_with_nesting(cursor, Direction::Current, 1, buf, '(', ')')?;
            (o, c, '(', ')')
        }
        '{' | '}' | 'B' => {
            let (o, c) = bracket_pair_with_nesting(cursor, Direction::Current, 1, buf, '{', '}')?;
            (o, c, '{', '}')
        }
        '[' | ']' | 'r' => {
            let (o, c) = bracket_pair_with_nesting(cursor, Direction::Current, 1, buf, '[', ']')?;
            (o, c, '[', ']')
        }
        '<' | '>' => {
            let (o, c) = bracket_pair_with_nesting(cursor, Direction::Current, 1, buf, '<', '>')?;
            (o, c, '<', '>')
        }
        '"' | '\'' | '`' => {
            let (o, c) = quote_pair_with_nesting(cursor, Direction::Current, 1, buf, ch)?;
            (o, c, ch, ch)
        }
        _ => return None,
    };

    let extra = count.max(1) - 1;
    let mut open_start = open_pos;
    for _ in 0..extra {
        let Some(prev) = open_start.checked_sub(1) else {
            break;
        };
        if buf.char_at(prev) != Some(Character::from(open_ch)) {
            break;
        }
        open_start = prev;
    }
    let mut close_end = close_pos;
    for _ in 0..extra {
        let next = close_end + 1;
        if buf.char_at(next) != Some(Character::from(close_ch)) {
            break;
        }
        close_end = next;
    }
    Some((open_start..open_pos + 1, close_pos..close_end + 1))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
