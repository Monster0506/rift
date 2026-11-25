use super::boundaries::*;

#[test]
fn test_next_word_basic() {
    let text = "hello world";
    assert_eq!(next_word(text, 0), 6); // "hello " -> "world"
    assert_eq!(next_word(text, 6), 11); // "world" -> end
}

#[test]
fn test_next_word_symbols() {
    let text = "foo->bar";
    assert_eq!(next_word(text, 0), 3); // "foo" -> "->"
    assert_eq!(next_word(text, 3), 5); // "->" -> "bar"
    assert_eq!(next_word(text, 5), 8); // "bar" -> end
}

#[test]
fn test_next_word_underscore() {
    let text = "hello_world";
    assert_eq!(next_word(text, 0), 11); // "hello_world" -> end (one word)
}

#[test]
fn test_next_word_multiple_spaces() {
    let text = "hello    world";
    assert_eq!(next_word(text, 0), 9); // "hello    " -> "world"
}

#[test]
fn test_prev_word_basic() {
    let text = "hello world";
    assert_eq!(prev_word(text, 11), 6); // end -> "world"
    assert_eq!(prev_word(text, 6), 0); // "world" -> "hello"
}

#[test]
fn test_prev_word_symbols() {
    let text = "foo->bar";
    assert_eq!(prev_word(text, 8), 5); // end -> "bar"
    assert_eq!(prev_word(text, 5), 3); // "bar" -> "->"
    assert_eq!(prev_word(text, 3), 0); // "->" -> "foo"
}

#[test]
fn test_prev_word_underscore() {
    let text = "hello_world";
    assert_eq!(prev_word(text, 11), 0); // end -> start (one word)
}

#[test]
fn test_prev_word_multiple_spaces() {
    let text = "hello    world";
    assert_eq!(prev_word(text, 14), 9); // end -> "world"
    assert_eq!(prev_word(text, 9), 0); // "world" -> "hello"
}

#[test]
fn test_edge_cases() {
    assert_eq!(next_word("", 0), 0);
    assert_eq!(prev_word("", 0), 0);
    assert_eq!(next_word("a", 1), 1);
    assert_eq!(prev_word("a", 0), 0);
}
