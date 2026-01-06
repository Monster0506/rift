use crate::buffer::rope::PieceTable;
use crate::character::Character;

fn create_table(text: &str) -> PieceTable {
    let mut pt = PieceTable::new(Vec::new());
    // Convert string to Characters
    let chars: Vec<Character> = text.chars().map(Character::from).collect();
    pt.insert(0, &chars);
    pt
}

#[test]
fn test_ascii_conversion() {
    let pt = create_table("Hello");
    // 'H' (0) -> byte 0
    // 'e' (1) -> byte 1
    // ...
    assert_eq!(pt.char_to_byte(0), 0);
    assert_eq!(pt.char_to_byte(1), 1);
    assert_eq!(pt.char_to_byte(5), 5); // End of buffer

    assert_eq!(pt.byte_to_char(0), 0);
    assert_eq!(pt.byte_to_char(1), 1);
    assert_eq!(pt.byte_to_char(5), 5);
}

#[test]
fn test_unicode_conversion() {
    // "A" (1 byte) + "🦀" (4 bytes) + "B" (1 byte)
    // Chars: 0='A', 1='🦀', 2='B'
    // Bytes: 0='A', 1..5='🦀', 5='B'
    let pt = create_table("A🦀B");

    // char -> byte
    assert_eq!(pt.char_to_byte(0), 0);
    assert_eq!(pt.char_to_byte(1), 1); // Start of crab
    assert_eq!(pt.char_to_byte(2), 5); // Start of B (1 + 4)
    assert_eq!(pt.char_to_byte(3), 6); // End (1 + 4 + 1)

    // byte -> char
    assert_eq!(pt.byte_to_char(0), 0);
    assert_eq!(pt.byte_to_char(1), 1);
    // Bytes 2, 3, 4 are inside the crab.
    // Usually mapping byte offset -> char index implies "which char contains this byte"
    // or "which char starts before or at this byte".
    // Let's assume strict boundaries for now, or "char containing".
    // Robust implementations often map interior bytes to the start of the char.
    assert_eq!(pt.byte_to_char(2), 1);
    assert_eq!(pt.byte_to_char(3), 1);
    assert_eq!(pt.byte_to_char(4), 1);
    assert_eq!(pt.byte_to_char(5), 2);
    assert_eq!(pt.byte_to_char(6), 3);
}

#[test]
fn test_complex_mixed() {
    // "a" (1) + "é" (2: c3 a9) + "€" (3: e2 82 ac)
    // Chars: 0='a', 1='é', 2='€'
    // Bytes: 0, 1(start é), 3(start €), 6(end)
    let pt = create_table("aé€");

    assert_eq!(pt.char_to_byte(0), 0);
    assert_eq!(pt.char_to_byte(1), 1);
    assert_eq!(pt.char_to_byte(2), 3);
    assert_eq!(pt.char_to_byte(3), 6);

    assert_eq!(pt.byte_to_char(0), 0);
    assert_eq!(pt.byte_to_char(1), 1);
    assert_eq!(pt.byte_to_char(2), 1); // inside é
    assert_eq!(pt.byte_to_char(3), 2);
    assert_eq!(pt.byte_to_char(4), 2); // inside €
    assert_eq!(pt.byte_to_char(5), 2); // inside €
    assert_eq!(pt.byte_to_char(6), 3);
}
