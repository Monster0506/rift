#[test]
fn test_unicode_boundary_movement() {
    let mut buf = crate::buffer::GapBuffer::new(100).unwrap();
    // '🦀' is F0 9F A6 80 (4 bytes)
    buf.insert_str("a🦀b").unwrap();
    buf.move_to_start();

    // Pos 0: |a🦀b
    assert_eq!(buf.cursor(), 0);

    // Move right 1 char -> should jump 'a' (1 byte)
    buf.move_right();
    println!("Cursor after move_right (b): {}", buf.cursor());
    assert_eq!(buf.cursor(), 1); // |🦀b

    // Move right 1 char -> should jump '🦀' (4 bytes)
    // 1 + 4 = 5
    buf.move_right();
    println!("Cursor after move_right (skip crab): {}", buf.cursor());
    assert_eq!(buf.cursor(), 5); // 🦀|b

    // Move right 1 char -> should jump 'b' (1 byte)
    buf.move_right();
    println!("Cursor after move_right (end): {}", buf.cursor());
    assert_eq!(buf.cursor(), 6); // 🦀b|

    // Move left -> should jump 'b' (1 byte)
    buf.move_left();
    println!("Cursor after move_left (b): {}", buf.cursor());
    assert_eq!(buf.cursor(), 5);

    // Move left -> should jump '🦀' (4 bytes)
    buf.move_left();
    println!("Cursor after move_left (crab): {}", buf.cursor());
    assert_eq!(buf.cursor(), 1);
}
