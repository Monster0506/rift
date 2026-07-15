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
fn test_to_logical_bytes_ascii() {
    let pt = create_table("Hello");
    let bytes = pt.to_logical_bytes();
    assert_eq!(bytes, b"Hello");
}

#[test]
fn test_to_logical_bytes_unicode() {
    let pt = create_table("🦀");
    let bytes = pt.to_logical_bytes();
    assert_eq!(bytes, "🦀".as_bytes());
}

#[test]
fn test_to_logical_bytes_mixed() {
    // \x01 renders as "^A" (2 bytes) but to_logical_bytes() yields 0x01 (1 byte).
    let mut pt = PieceTable::new(Vec::new());
    pt.insert(0, &[Character::Control(1)]); // ^A

    let bytes = pt.to_logical_bytes();
    assert_eq!(bytes, vec![1]);

    // Verify string representation is different (simulating the bug)
    let s = pt.to_string();
    assert_eq!(s, "^A");
    assert_ne!(bytes, s.as_bytes());
}
