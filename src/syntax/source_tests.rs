use super::*;
use crate::syntax::loader::LanguageLoader;
use tree_sitter::InputEdit;

fn parser(language: &str) -> Parser {
    let grammar = LanguageLoader::new(std::path::PathBuf::default())
        .load_language(language)
        .unwrap();
    let mut parser = Parser::new();
    parser.set_language(&grammar.language).unwrap();
    parser
}

fn fragmented(parts: &[&[u8]]) -> TextBuffer {
    let mut buffer = TextBuffer::new(256).unwrap();
    // Prepending keeps successive additions in separate pieces.
    for part in parts.iter().rev() {
        assert!(crate::document::ops::standalone::place_cursor(
            &mut buffer,
            0
        ));
        crate::document::ops::Edit::type_text(crate::buffer::decode_bytes(part))
            .apply_to(&mut buffer);
    }
    assert!(buffer.iter_chunks_at(0).count() >= parts.len());
    assert_eq!(buffer.to_logical_bytes(), parts.concat());
    buffer
}

fn mixed_buffer(language: &str) -> TextBuffer {
    let (prefix, suffix): (&[u8], &[u8]) = match language {
        "rust" => (
            b"fn main() {\n    let title = \"",
            b"\";\n    let items = [1, 2, 3];\n}\n",
        ),
        "json" => (
            b"{\n  \"title\": \"",
            b"\",\n  \"items\": [1, true, null]\n}\n",
        ),
        _ => unreachable!(),
    };
    fragmented(&[
        prefix,
        "café".as_bytes(),
        b" / ",
        "漢🦀\u{0085}\u{009f}".as_bytes(),
        &[0xff, 0x80, 0xc0, 0xaf, 0xe2, b'(', 0xa1, 0xf0, 0x9f],
        suffix,
    ])
}

fn point(bytes: &[u8], byte: usize) -> Point {
    let prefix = &bytes[..byte];
    Point {
        row: prefix.split(|&b| b == b'\n').count() - 1,
        column: prefix
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(byte, |newline| byte - newline - 1),
    }
}

fn assert_trees_equal(actual: &Tree, expected: &Tree, buffer: &TextBuffer) {
    let bytes = buffer.to_logical_bytes();
    let source = Source::new(buffer);
    let mut actual = actual.walk();
    let mut expected = expected.walk();
    loop {
        let node = actual.node();
        let reference = expected.node();
        assert_eq!(node.kind(), reference.kind());
        assert_eq!(node.kind_id(), reference.kind_id());
        assert_eq!(node.is_named(), reference.is_named());
        assert_eq!(node.is_extra(), reference.is_extra());
        assert_eq!(node.is_error(), reference.is_error());
        assert_eq!(node.is_missing(), reference.is_missing());
        assert_eq!(node.child_count(), reference.child_count());
        assert_eq!(actual.field_name(), expected.field_name());
        assert_eq!(node.byte_range(), reference.byte_range());
        assert_eq!(node.start_position(), reference.start_position());
        assert_eq!(node.end_position(), reference.end_position());
        assert_eq!(node.start_position(), point(&bytes, node.start_byte()));
        assert_eq!(node.end_position(), point(&bytes, node.end_byte()));
        assert_eq!(source.point(node.start_byte()), node.start_position());
        assert_eq!(source.point(node.end_byte()), node.end_position());
        assert_eq!(source.text(node.byte_range()), bytes[node.byte_range()]);

        let child = actual.goto_first_child();
        assert_eq!(child, expected.goto_first_child());
        if child {
            continue;
        }
        loop {
            let sibling = actual.goto_next_sibling();
            assert_eq!(sibling, expected.goto_next_sibling());
            if sibling {
                break;
            }
            let parent = actual.goto_parent();
            assert_eq!(parent, expected.goto_parent());
            if !parent {
                return;
            }
        }
    }
}

#[test]
fn callback_parse_matches_logical_bytes_for_fragmented_mixed_input() {
    for language in ["rust", "json"] {
        let buffer = mixed_buffer(language);
        let mut parser = parser(language);
        let callback = Source::new(&buffer).parse(&mut parser, None, None).unwrap();
        let reference = parser.parse(buffer.to_logical_bytes(), None).unwrap();
        assert_trees_equal(&callback, &reference, &buffer);
    }
}

#[test]
fn text_matches_logical_byte_slices_even_inside_codepoints() {
    let buffer = mixed_buffer("json");
    let bytes = buffer.to_logical_bytes();
    let source = Source::new(&buffer);
    for start in 0..=bytes.len() {
        assert_eq!(source.point(start), point(&bytes, start));
        for end in start..=bytes.len() {
            assert_eq!(
                source.text(start..end),
                bytes[start..end],
                "logical byte range {start}..{end}"
            );
        }
    }
    assert_eq!(
        source.text(bytes.len() - 2..bytes.len() + 8),
        bytes[bytes.len() - 2..]
    );
    assert_eq!(source.text(bytes.len()..bytes.len() + 8), Vec::<u8>::new());
    assert_eq!(
        source.text(bytes.len() + 1..bytes.len() + 8),
        Vec::<u8>::new()
    );
}

#[test]
fn callback_parse_crosses_multibyte_read_boundaries() {
    let prefix = b"{\"payload\":\"";
    let first_padding = vec![b'a'; READ_BYTES - prefix.len() - 1];
    let second_padding = vec![b'b'; READ_BYTES - "🦀".len()];
    let buffer = fragmented(&[
        prefix,
        &first_padding,
        "🦀".as_bytes(),
        &second_padding,
        "漢".as_bytes(),
        b"\",\n\"next\": [true, false, null, {\"count\": 7}]}\n",
    ]);
    let bytes = buffer.to_logical_bytes();
    let source = Source::new(&buffer);
    assert_eq!(source.encode_from(0, READ_BYTES), bytes[..READ_BYTES + 3]);
    for start in [
        READ_BYTES - 1,
        READ_BYTES,
        READ_BYTES + 1,
        2 * READ_BYTES - 1,
    ] {
        let chunk = source.encode_from(start, READ_BYTES);
        assert_eq!(chunk, bytes[start..start + chunk.len()]);
        assert!(chunk.len() >= READ_BYTES.min(bytes.len() - start));
    }
    let mut parser = parser("json");
    let callback = source.parse(&mut parser, None, None).unwrap();
    let reference = parser.parse(&bytes, None).unwrap();
    assert!(!reference.root_node().has_error());
    assert_trees_equal(&callback, &reference, &buffer);
}

fn find(bytes: &[u8], needle: &[u8]) -> usize {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap()
}

fn edit_and_reparse(
    buffer: &mut TextBuffer,
    parser: &mut Parser,
    tree: &mut Tree,
    range: Range<usize>,
    replacement: &[u8],
) {
    let before = buffer.to_logical_bytes();
    let start_char = buffer.byte_to_char(range.start);
    let end_char = buffer.byte_to_char(range.end);
    assert_eq!(buffer.char_to_byte(start_char), range.start);
    assert_eq!(buffer.char_to_byte(end_char), range.end);
    if start_char != end_char {
        assert!(
            crate::document::ops::Edit::delete(start_char..start_char + end_char - start_char)
                .apply_to(buffer)
        );
    }
    assert!(crate::document::ops::standalone::place_cursor(
        buffer, start_char
    ));
    crate::document::ops::Edit::type_text(crate::buffer::decode_bytes(replacement))
        .apply_to(buffer);
    let after = buffer.to_logical_bytes();
    let mut expected_bytes = before.clone();
    expected_bytes.splice(range.clone(), replacement.iter().copied());
    assert_eq!(after, expected_bytes);
    let new_end_byte = range.start + replacement.len();
    tree.edit(&InputEdit {
        start_byte: range.start,
        old_end_byte: range.end,
        new_end_byte,
        start_position: point(&before, range.start),
        old_end_position: point(&before, range.end),
        new_end_position: point(&after, new_end_byte),
    });
    let callback = Source::new(buffer).parse(parser, Some(tree), None).unwrap();
    let reference = parser.parse(&after, None).unwrap();
    assert_trees_equal(&callback, &reference, buffer);
    *tree = callback;
}

#[test]
fn callback_reparse_matches_fresh_parse_after_byte_and_multiline_edits() {
    let mut buffer = mixed_buffer("json");
    let mut parser = parser("json");
    let mut tree = Source::new(&buffer).parse(&mut parser, None, None).unwrap();

    let bytes = buffer.to_logical_bytes();
    let start = find(&bytes, b"[1, true, null]");
    edit_and_reparse(
        &mut buffer,
        &mut parser,
        &mut tree,
        start..start + b"[1, true, null]".len(),
        "[\n    {\"名\": \"é🦀\"},\n    42\n  ]".as_bytes(),
    );

    let bytes = buffer.to_logical_bytes();
    let start = find(&bytes, "café".as_bytes()) + "café".len();
    edit_and_reparse(
        &mut buffer,
        &mut parser,
        &mut tree,
        start..start,
        &[0xc2, 0x85, 0xff, 0x80],
    );

    let bytes = buffer.to_logical_bytes();
    let start = find(&bytes, "漢🦀".as_bytes());
    edit_and_reparse(
        &mut buffer,
        &mut parser,
        &mut tree,
        start..start + "漢🦀".len(),
        b"",
    );
}
