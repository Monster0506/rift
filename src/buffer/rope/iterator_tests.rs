use crate::buffer::rope::PieceTable;
use crate::character::Character;

fn create_table(text: &str) -> PieceTable {
    let mut pt = PieceTable::new(Vec::new());
    if !text.is_empty() {
        let chars: Vec<Character> = text.chars().map(Character::from).collect();
        pt.insert(0, &chars);
    }
    pt
}

#[test]
fn test_iterator_empty() {
    let pt = create_table("");
    let mut iter = pt.iter();
    assert_eq!(iter.next(), None);
}

#[test]
fn test_iterator_simple() {
    let pt = create_table("abc");
    let mut iter = pt.iter();
    assert_eq!(iter.next(), Some(Character::from('a')));
    assert_eq!(iter.next(), Some(Character::from('b')));
    assert_eq!(iter.next(), Some(Character::from('c')));
    assert_eq!(iter.next(), None);
}

#[test]
fn test_iterator_complex_inserts() {
    // "Hello" -> insert " World" at 5 -> "Hello World"
    let mut pt = create_table("Hello");
    let world: Vec<Character> = " World".chars().map(Character::from).collect();
    pt.insert(5, &world);

    let collected: String = pt.iter().map(|c| c.to_char_lossy()).collect();
    assert_eq!(collected, "Hello World");
}

#[test]
fn test_iterator_with_deletes() {
    // "Hello World" -> delete " World" -> "Hello"
    let mut pt = create_table("Hello World");
    pt.delete(5..11);

    let collected: String = pt.iter().map(|c| c.to_char_lossy()).collect();
    assert_eq!(collected, "Hello");
}

#[test]
fn test_iter_at() {
    let pt = create_table("012345");
    let mut iter = pt.iter_at(3);
    assert_eq!(iter.next(), Some(Character::from('3')));
    assert_eq!(iter.next(), Some(Character::from('4')));
    assert_eq!(iter.next(), Some(Character::from('5')));
    assert_eq!(iter.next(), None);
}
