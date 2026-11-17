use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use std::ops::Range;
use tree_sitter::{Node, ParseOptions, Parser, Point, Tree};

/// Bytes handed to tree-sitter per read callback.
const READ_BYTES: usize = 16 * 1024;

/// Logical-byte view of a `TextBuffer` for tree-sitter, read piecewise so
/// parsing and querying never copy the whole document.
#[derive(Clone, Copy)]
pub(super) struct Source<'a> {
    buffer: &'a TextBuffer,
    len: usize,
}

impl<'a> Source<'a> {
    pub(super) fn new(buffer: &'a TextBuffer) -> Self {
        Self {
            buffer,
            len: buffer.byte_len(),
        }
    }

    pub(super) const fn len(self) -> usize {
        self.len
    }

    pub(super) fn parse(
        self,
        parser: &mut Parser,
        old_tree: Option<&Tree>,
        options: Option<ParseOptions>,
    ) -> Option<Tree> {
        parser.parse_with_options(
            &mut |byte, _| self.encode_from(byte, READ_BYTES),
            old_tree,
            options,
        )
    }

    /// Text provider for query predicates (`#match?`, `#eq?`).
    pub(super) fn node_text(self) -> impl FnMut(Node) -> std::iter::Once<Vec<u8>> + 'a {
        move |node| std::iter::once(self.text(node.byte_range()))
    }

    pub(super) fn text(self, range: Range<usize>) -> Vec<u8> {
        let len = range.end.min(self.len).saturating_sub(range.start);
        let mut out = self.encode_from(range.start, len);
        out.truncate(len);
        out
    }

    pub(super) fn point(self, byte: usize) -> Point {
        let byte = byte.min(self.len);
        let index = &self.buffer.line_index;
        let row = index.get_line_at(self.buffer.byte_to_char(byte));
        let line_start = self.buffer.char_to_byte(index.get_line_start(row));
        Point {
            row,
            column: byte - line_start,
        }
    }

    pub(super) fn line_start(self, byte: usize) -> usize {
        byte - self.point(byte).column
    }

    /// First line start at or after `byte`, or the document end.
    pub(super) fn next_line_start(self, byte: usize) -> usize {
        if byte >= self.len {
            return self.len;
        }
        let point = self.point(byte);
        if point.column == 0 {
            return byte;
        }
        self.buffer
            .line_index
            .get_start(point.row + 1)
            .map_or(self.len, |c| self.buffer.char_to_byte(c))
    }

    /// At least `min_len` bytes from `byte` (fewer at the end), starting
    /// mid-character if `byte` falls inside one.
    fn encode_from(self, byte: usize, min_len: usize) -> Vec<u8> {
        if byte >= self.len || min_len == 0 {
            return Vec::new();
        }
        let first = self.buffer.byte_to_char(byte);
        let skip = byte - self.buffer.char_to_byte(first);
        let want = skip + min_len;

        let mut out = Vec::with_capacity(want.min(self.len - byte + skip));
        'pieces: for piece in self.buffer.iter_chunks_at(first) {
            for &ch in piece {
                ch.encode_utf8(&mut out);
                if out.len() >= want {
                    break 'pieces;
                }
            }
        }

        out.drain(..skip);
        out
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
