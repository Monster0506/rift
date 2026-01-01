use crate::buffer::TextBuffer;

#[test]
fn test_text_provider_chunks() {
    let mut buffer = TextBuffer::new(100).unwrap();
    buffer.insert_str("line1\nline2\nline3").unwrap();

    // Test collecting chunks from the provider
    let range = 0..buffer.len();
    let chunks: Vec<Vec<u8>> = buffer
        .line_index
        .chunks_in_range(range)
        .map(|c| c.to_vec())
        .collect();

    let flattened: Vec<u8> = chunks.into_iter().flatten().collect();
    assert_eq!(String::from_utf8(flattened).unwrap(), "line1\nline2\nline3");
}

#[test]
fn test_syntax_new_placeholder() {
    // Basic test to ensure TextBuffer is usable
    let buffer = TextBuffer::new(10).unwrap();
    assert_eq!(buffer.len(), 0);
}
