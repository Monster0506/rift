use super::*;

#[test]
fn next_word_basic() {
    assert_eq!(next_word("hello world", 0), 6);
    assert_eq!(next_word("foo->bar", 0), 3);
}

#[test]
fn prev_word_basic() {
    assert_eq!(prev_word("hello world", 11), 6);
    assert_eq!(prev_word("foo->bar", 8), 5);
}
