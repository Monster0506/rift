use crate::buffer::TextBuffer;

#[test]
fn test_text_provider_chunks() {
    let mut buffer = TextBuffer::new(100).unwrap();
    buffer.insert_str("line1\nline2\nline3").unwrap();

    // Test collecting chunks from the provider - replaced by to_string check
    // since chunks_in_range was removed
    assert_eq!(buffer.to_string(), "line1\nline2\nline3");
}

#[test]
fn test_syntax_new_placeholder() {
    // Basic test to ensure TextBuffer is usable
    let buffer = TextBuffer::new(10).unwrap();
    assert_eq!(buffer.len(), 0);
}
