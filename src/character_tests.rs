use super::*;

#[test]
fn test_encode_utf8_unicode() {
    let c = Character::Unicode('🦀');
    let mut buf = Vec::new();
    c.encode_utf8(&mut buf);
    assert_eq!(buf, "🦀".as_bytes());
}

#[test]
fn test_encode_utf8_ascii() {
    let c = Character::Unicode('a');
    let mut buf = Vec::new();
    c.encode_utf8(&mut buf);
    assert_eq!(buf, b"a");
}

#[test]
fn test_encode_utf8_special() {
    let nl = Character::Newline;
    let mut buf = Vec::new();
    nl.encode_utf8(&mut buf);
    assert_eq!(buf, b"\n");

    let tab = Character::Tab;
    buf.clear();
    tab.encode_utf8(&mut buf);
    assert_eq!(buf, b"\t");

    let byte = Character::Byte(0xFF);
    buf.clear();
    byte.encode_utf8(&mut buf);
    assert_eq!(buf, vec![0xFF]);

    let ctrl = Character::Control(0x01);
    buf.clear();
    ctrl.encode_utf8(&mut buf);
    assert_eq!(buf, vec![0x01]);
}
