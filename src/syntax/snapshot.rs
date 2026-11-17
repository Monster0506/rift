use super::source::Source;
use crate::buffer::TextBuffer;
use std::ops::Range;
use std::rc::Rc;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, QueryMatch, Tree};

pub(crate) enum SnapshotText {
    Buffer(TextBuffer),
    Owned(String),
}

impl SnapshotText {
    #[must_use]
    pub(crate) fn text(&self, range: Range<usize>) -> Vec<u8> {
        match self {
            Self::Buffer(buffer) => Source::new(buffer).text(range),
            Self::Owned(text) => {
                let end = range.end.min(text.len());
                text.as_bytes()
                    .get(range.start.min(end)..end)
                    .unwrap_or_default()
                    .to_vec()
            }
        }
    }

    #[must_use]
    pub(crate) fn byte_at(&self, row: usize, col: usize) -> usize {
        let line = self.line_bytes(row);
        line.start + col.min(line.len())
    }

    #[must_use]
    pub(crate) fn byte_col(&self, row: usize, char_col: usize) -> usize {
        let text = self.text(self.line_bytes(row));
        String::from_utf8_lossy(&text)
            .char_indices()
            .nth(char_col)
            .map_or(text.len(), |(i, _)| i)
    }

    fn line_bytes(&self, row: usize) -> Range<usize> {
        match self {
            Self::Buffer(buffer) => buffer.line_byte_range(row),
            Self::Owned(text) => {
                let mut start = 0;
                for _ in 0..row {
                    match text[start..].find('\n') {
                        Some(i) => start += i + 1,
                        None => break,
                    }
                }
                let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
                start..end.max(start)
            }
        }
    }
}

pub(crate) struct SyntaxSnapshot {
    pub(crate) buf: Option<u64>,
    pub(crate) lang_name: String,
    tree: Tree,
    pub(crate) text: Rc<SnapshotText>,
}

impl SyntaxSnapshot {
    #[must_use]
    pub(crate) const fn new(
        buf: Option<u64>,
        lang_name: String,
        tree: Tree,
        text: Rc<SnapshotText>,
    ) -> Self {
        Self {
            buf,
            lang_name,
            tree,
            text,
        }
    }

    #[must_use]
    pub(crate) fn parse_string(
        lang_name: String,
        language: &Language,
        text: String,
    ) -> Option<Self> {
        let mut parser = Parser::new();
        parser.set_language(language).ok()?;
        let tree = parser.parse(&text, None)?;
        Some(Self::new(
            None,
            lang_name,
            tree,
            Rc::new(SnapshotText::Owned(text)),
        ))
    }

    #[must_use]
    pub(crate) fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    #[must_use]
    pub(crate) fn node_text(&self, node: Node<'_>) -> Vec<u8> {
        self.text.text(node.byte_range())
    }

    pub(crate) fn for_each_match<'s>(
        &'s self,
        query: &Query,
        node: Node<'s>,
        window: Option<QueryWindow>,
        mut f: impl FnMut(&QueryMatch<'_, 's>),
    ) {
        use streaming_iterator::StreamingIterator;
        let mut cursor = Self::cursor(window);
        let text = &self.text;
        let mut matches = cursor.matches(query, node, |n: Node<'_>| {
            std::iter::once(text.text(n.byte_range()))
        });
        while let Some(m) = matches.next() {
            f(m);
        }
    }

    pub(crate) fn for_each_capture<'s>(
        &'s self,
        query: &Query,
        node: Node<'s>,
        window: Option<QueryWindow>,
        mut f: impl FnMut(&QueryMatch<'_, 's>, usize),
    ) {
        use streaming_iterator::StreamingIterator;
        let mut cursor = Self::cursor(window);
        let text = &self.text;
        let mut captures = cursor.captures(query, node, |n: Node<'_>| {
            std::iter::once(text.text(n.byte_range()))
        });
        while let Some((m, index)) = captures.next() {
            f(m, *index);
        }
    }

    fn cursor(window: Option<QueryWindow>) -> QueryCursor {
        let mut cursor = QueryCursor::new();
        match window {
            Some(QueryWindow::Rows(rows)) => {
                cursor.set_point_range(
                    tree_sitter::Point::new(rows.start, 0)..tree_sitter::Point::new(rows.end, 0),
                );
            }
            Some(QueryWindow::Bytes(bytes)) => {
                cursor.set_byte_range(bytes);
            }
            None => {}
        }
        cursor
    }
}

pub(crate) enum QueryWindow {
    Rows(Range<usize>),
    Bytes(Range<usize>),
}

pub(crate) struct BufferSyntax {
    pub(crate) host: Rc<SyntaxSnapshot>,
    pub(crate) injected: Vec<Rc<SyntaxSnapshot>>,
}
