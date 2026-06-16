use super::{charwise_inclusive, is_space_or_tab, BaseModifier, ObjectKind};
use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::wrap::MotionRange;
use tree_sitter::{Node, Tree};

/// A parsed buffer's tree-sitter state, `source` must be the same byte representation the tree was parsed from
pub struct SyntaxContext<'a> {
    pub tree: &'a Tree,
    pub source: &'a [u8],
}

const BRACKET_TRIM_PAIRS: [(char, char); 3] = [('(', ')'), ('{', '}'), ('[', ']')];

fn is_call(kind: &str) -> bool {
    kind.contains("call")
}

fn is_argument_container(kind: &str) -> bool {
    kind.contains("argument") || kind.contains("parameter")
}

fn is_function_def(kind: &str) -> bool {
    (kind.contains("function") || kind.contains("method"))
        && !kind.contains("call")
        && !kind.contains("type")
}

fn is_class(kind: &str) -> bool {
    kind.contains("class") || kind.contains("struct") || kind.contains("impl")
}

fn is_block(kind: &str) -> bool {
    kind.contains("block")
        || kind == "body"
        || kind.contains("declaration_list")
        || kind.contains("compound_statement")
        || kind.contains("suite")
}

fn is_tag_element(kind: &str) -> bool {
    kind.contains("element")
}

fn is_number_literal(kind: &str) -> bool {
    kind.contains("integer")
        || kind.contains("float")
        || kind.contains("number")
        || kind.contains("numeric")
}

fn node_at_cursor<'tree>(ctx: &SyntaxContext<'tree>, cursor_byte: usize) -> Node<'tree> {
    let len = ctx.source.len();
    let start = cursor_byte.min(len);
    let end = (cursor_byte + 1).min(len).max(start);
    ctx.tree
        .root_node()
        .descendant_for_byte_range(start, end)
        .unwrap_or_else(|| ctx.tree.root_node())
}

/// Walks up from `node` to the n-th ancestor (inclusive of `node` itself)
/// matching `pred`, supporting nest-count for tree-sitter objects.
fn find_ancestor_nth<'tree>(
    node: Node<'tree>,
    pred: impl Fn(&str) -> bool,
    n: u8,
) -> Option<Node<'tree>> {
    let mut remaining = n.max(1) as i32;
    let mut cur = Some(node);
    while let Some(nd) = cur {
        if pred(nd.kind()) {
            remaining -= 1;
            if remaining <= 0 {
                return Some(nd);
            }
        }
        cur = nd.parent();
    }
    None
}

fn node_char_range(node: Node, buf: &TextBuffer) -> Option<(usize, usize)> {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    if end_byte <= start_byte {
        return None;
    }
    let start = buf.byte_to_char(start_byte);
    let end = buf.byte_to_char(end_byte - 1);
    Some((start, end))
}

/// Trims a matching outer bracket pair from a char range, if present.
fn strip_outer_brackets(start: usize, end: usize, buf: &TextBuffer) -> Option<(usize, usize)> {
    if start >= end {
        return Some((start, end));
    }
    let (Some(first), Some(last)) = (buf.char_at(start), buf.char_at(end)) else {
        return Some((start, end));
    };
    for &(open, close) in BRACKET_TRIM_PAIRS.iter() {
        if first == Character::from(open) && last == Character::from(close) {
            if start + 1 > end - 1 {
                return None;
            }
            return Some((start + 1, end - 1));
        }
    }
    Some((start, end))
}

pub(super) fn resolve(
    kind: ObjectKind,
    modifier: BaseModifier,
    nesting: u8,
    cursor: usize,
    buf: &TextBuffer,
    ctx: &SyntaxContext,
) -> Option<MotionRange> {
    let cursor_byte = buf.char_to_byte(cursor);
    let node = node_at_cursor(ctx, cursor_byte);
    match kind {
        ObjectKind::FunctionCall => resolve_function_call(node, modifier, nesting, buf),
        ObjectKind::Argument => resolve_argument(node, modifier, buf),
        ObjectKind::FunctionDef => {
            resolve_body_owner(node, modifier, nesting, is_function_def, buf)
        }
        ObjectKind::Class => resolve_body_owner(node, modifier, nesting, is_class, buf),
        ObjectKind::Block => resolve_block(node, modifier, nesting, buf),
        ObjectKind::Tag => resolve_tag(node, modifier, nesting, buf),
        ObjectKind::Number => resolve_number(node, nesting, buf),
        _ => None,
    }
}

fn resolve_function_call(
    node: Node,
    modifier: BaseModifier,
    nesting: u8,
    buf: &TextBuffer,
) -> Option<MotionRange> {
    let call = find_ancestor_nth(node, is_call, nesting)?;
    let mut walker = call.walk();
    let args = call
        .named_children(&mut walker)
        .find(|c| is_argument_container(c.kind()))?;
    let (start, end) = node_char_range(args, buf)?;
    match modifier {
        BaseModifier::Around => Some(charwise_inclusive(start, end)),
        BaseModifier::Inner => {
            let (s, e) = strip_outer_brackets(start, end, buf)?;
            Some(charwise_inclusive(s, e))
        }
    }
}

fn resolve_argument(node: Node, modifier: BaseModifier, buf: &TextBuffer) -> Option<MotionRange> {
    let mut cur = node;
    let item = loop {
        let parent = cur.parent()?;
        if is_argument_container(parent.kind()) {
            break cur;
        }
        cur = parent;
    };
    let (start, end) = node_char_range(item, buf)?;
    match modifier {
        BaseModifier::Inner => Some(charwise_inclusive(start, end)),
        BaseModifier::Around => Some(around_argument(start, end, buf)),
    }
}

/// Eats a trailing comma (and following whitespace), falling back to a
/// leading comma when this is the last argument in the list.
fn around_argument(start: usize, end: usize, buf: &TextBuffer) -> MotionRange {
    let len = buf.len();
    let mut pos = end + 1;
    while pos < len {
        match buf.char_at(pos) {
            Some(ch) if is_space_or_tab(ch) => pos += 1,
            _ => break,
        }
    }
    if buf.char_at(pos) == Some(Character::from(',')) {
        let mut aend = pos;
        while aend + 1 < len {
            match buf.char_at(aend + 1) {
                Some(ch) if is_space_or_tab(ch) => aend += 1,
                _ => break,
            }
        }
        return charwise_inclusive(start, aend);
    }
    if start > 0 {
        let mut p = start - 1;
        while p > 0 {
            match buf.char_at(p) {
                Some(ch) if is_space_or_tab(ch) => p -= 1,
                _ => break,
            }
        }
        if buf.char_at(p) == Some(Character::from(',')) {
            return charwise_inclusive(p, end);
        }
    }
    charwise_inclusive(start, end)
}

/// Shared resolver for objects that are "an ancestor node with a body block":
/// `aX` selects the whole node, `iX` selects its body block's content.
fn resolve_body_owner(
    node: Node,
    modifier: BaseModifier,
    nesting: u8,
    is_owner: impl Fn(&str) -> bool,
    buf: &TextBuffer,
) -> Option<MotionRange> {
    let owner = find_ancestor_nth(node, is_owner, nesting)?;
    match modifier {
        BaseModifier::Around => {
            let (start, end) = node_char_range(owner, buf)?;
            Some(charwise_inclusive(start, end))
        }
        BaseModifier::Inner => {
            let mut walker = owner.walk();
            let body = owner
                .named_children(&mut walker)
                .find(|c| is_block(c.kind()))?;
            let (start, end) = node_char_range(body, buf)?;
            let (s, e) = strip_outer_brackets(start, end, buf)?;
            Some(charwise_inclusive(s, e))
        }
    }
}

fn resolve_block(
    node: Node,
    modifier: BaseModifier,
    nesting: u8,
    buf: &TextBuffer,
) -> Option<MotionRange> {
    let block = find_ancestor_nth(node, is_block, nesting)?;
    let (start, end) = node_char_range(block, buf)?;
    match modifier {
        BaseModifier::Around => Some(charwise_inclusive(start, end)),
        BaseModifier::Inner => {
            let (s, e) = strip_outer_brackets(start, end, buf)?;
            Some(charwise_inclusive(s, e))
        }
    }
}

fn resolve_tag(
    node: Node,
    modifier: BaseModifier,
    nesting: u8,
    buf: &TextBuffer,
) -> Option<MotionRange> {
    let element = find_ancestor_nth(node, is_tag_element, nesting)?;
    let (start, end) = node_char_range(element, buf)?;
    if modifier == BaseModifier::Around {
        return Some(charwise_inclusive(start, end));
    }
    let mut walker = element.walk();
    let children: Vec<Node> = element.children(&mut walker).collect();
    let open_tag = children.iter().find(|c| c.kind().contains("start_tag"))?;
    let close_tag = children.iter().find(|c| c.kind().contains("end_tag"))?;
    if close_tag.start_byte() <= open_tag.end_byte() {
        return None;
    }
    let inner_start = buf.byte_to_char(open_tag.end_byte());
    let inner_end = buf.byte_to_char(close_tag.start_byte() - 1);
    Some(charwise_inclusive(inner_start, inner_end))
}

fn resolve_number(node: Node, nesting: u8, buf: &TextBuffer) -> Option<MotionRange> {
    let lit = find_ancestor_nth(node, is_number_literal, nesting)?;
    let (start, end) = node_char_range(lit, buf)?;
    Some(charwise_inclusive(start, end))
}
