use super::*;
use crate::key::notation::format_key;
use crate::key::notation::parse_key;
use crate::key::notation::parse_key_sequence;

const ALL_MODS: [Mods; 8] = [
    Mods::NONE,
    Mods::SHIFT,
    Mods::CTRL,
    Mods::ALT,
    Mods(1 | 2),
    Mods(1 | 4),
    Mods(2 | 4),
    Mods(1 | 2 | 4),
];

fn every_key() -> Vec<Key> {
    let mut codes = vec![
        KeyCode::Insert,
        KeyCode::Enter,
        KeyCode::Esc,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Delete,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
    ];
    codes.extend((1..=24).map(KeyCode::F));
    codes.extend(
        [
            'a', 'Z', ' ', '<', '>', '-', '1', '[', '\\', 'ł', '€', '\0', '🦀',
        ]
        .map(KeyCode::Char),
    );

    codes
        .into_iter()
        .flat_map(|code| ALL_MODS.map(|mods| Key::new(code, mods)))
        .collect()
}

#[test]
fn notation_round_trips_every_key() {
    for key in every_key() {
        let text = format_key(key);
        assert_eq!(parse_key(&text), Some(key), "{key:?} via {text:?}");
        assert_eq!(parse_key_sequence(&text), Some(vec![key]), "{text:?}");
    }
}

#[test]
fn notation_format_examples() {
    assert_eq!(format_key(Key::char('a')), "a");
    assert_eq!(format_key(Key::SPACE), "<Space>");
    assert_eq!(format_key(Key::char('<')), "<lt>");
    assert_eq!(format_key(Key::ctrl('a')), "<C-a>");
    assert_eq!(format_key(Key::SHIFT_SPACE), "<S-Space>");
    assert_eq!(
        format_key(Key::LEFT.with(Mods::CTRL | Mods::ALT)),
        "<C-A-Left>"
    );
    assert_eq!(format_key(Key::f(5)), "<F5>");
    assert_eq!(format_key(Key::char('\0')), "<u+0000>");
}

#[test]
fn notation_parses_modifier_chains_and_names() {
    let all = Mods::CTRL | Mods::SHIFT | Mods::ALT;
    assert_eq!(
        parse_key("<C-S-A-Left>"),
        Some(Key::new(KeyCode::Left, all))
    );
    assert_eq!(parse_key("<c-s-a-left>"), parse_key("<C-S-A-Left>"));
    assert_eq!(parse_key("<C-Space>"), Some(Key::ctrl(' ')));
    assert_eq!(parse_key("<S-Space>"), Some(Key::SHIFT_SPACE));
    assert_eq!(parse_key("<F5>"), Some(Key::f(5)));
    assert_eq!(
        parse_key("<C-F12>"),
        Some(Key::new(KeyCode::F(12), Mods::CTRL))
    );
    assert_eq!(parse_key("<C-ł>"), Some(Key::ctrl('ł')));
    assert_eq!(parse_key("<C-->"), Some(Key::ctrl('-')));
    assert_eq!(parse_key("<C-lt>"), Some(Key::ctrl('<')));
    assert_eq!(parse_key("<CR>"), Some(Key::ENTER));
    assert_eq!(parse_key("<BS>"), Some(Key::BACKSPACE));
}

#[test]
fn notation_rejects_garbage() {
    for bad in [
        "",
        "<>",
        "<C->",
        "<F0>",
        "<F25>",
        "<Bogus>",
        "<C-Bogus>",
        "ab",
        "<",
    ] {
        assert_eq!(parse_key(bad), None, "{bad:?}");
    }
    assert_eq!(parse_key_sequence("<"), None);
    assert_eq!(parse_key_sequence("<Bogus>"), None);
    assert_eq!(parse_key_sequence(""), None);
}

#[test]
fn sequences_mix_plain_and_bracketed_keys() {
    assert_eq!(
        parse_key_sequence("d<lt>>"),
        Some(vec![Key::char('d'), Key::char('<'), Key::char('>')])
    );
    assert_eq!(parse_key_sequence("<A-P>"), Some(vec![Key::alt('p')]));
    assert_eq!(
        parse_key_sequence("<C-S-w>x"),
        Some(vec![
            Key::new(KeyCode::Char('w'), Mods::CTRL | Mods::SHIFT),
            Key::char('x')
        ])
    );
}

#[test]
fn canonical_form_drops_shift_on_plain_chars_but_not_space() {
    assert_eq!(Key::new(KeyCode::Char('a'), Mods::SHIFT), Key::char('A'));
    assert_eq!(Key::new(KeyCode::Char('!'), Mods::SHIFT), Key::char('!'));
    assert_eq!(Key::new(KeyCode::Char(' '), Mods::SHIFT), Key::SHIFT_SPACE);
    assert_ne!(Key::SHIFT_SPACE, Key::SPACE);
}

#[test]
fn canonical_form_lowercases_ctrl_and_alt_letters() {
    assert_eq!(Key::new(KeyCode::Char('H'), Mods::CTRL), Key::ctrl('h'));
    assert_eq!(Key::new(KeyCode::Char('H'), Mods::ALT), Key::alt('h'));
    let ctrl_shift = Key::new(KeyCode::Char('h'), Mods::CTRL | Mods::SHIFT);
    assert_ne!(ctrl_shift, Key::ctrl('h'));
}

#[test]
fn legacy_control_collisions_collapse_to_named_keys() {
    assert_eq!(Key::ctrl('i'), Key::TAB);
    assert_eq!(Key::ctrl('m'), Key::ENTER);
    assert_eq!(Key::ctrl('['), Key::ESC);
    assert_ne!(Key::ctrl('h'), Key::BACKSPACE);
}

#[test]
fn text_is_only_for_typed_characters() {
    assert_eq!(Key::char('a').text(), Some('a'));
    assert_eq!(Key::SHIFT_SPACE.text(), Some(' '));
    assert_eq!(Key::ctrl('a').text(), None);
    assert_eq!(Key::alt('a').text(), None);
    assert_eq!(Key::char('\0').text(), None);
    assert_eq!(Key::ENTER.text(), None);
}

#[test]
fn fallback_strips_only_modifiers_without_text_meaning() {
    assert_eq!(Key::SHIFT_SPACE.fallback(), Some(Key::SPACE));
    assert_eq!(Key::LEFT.with(Mods::SHIFT).fallback(), Some(Key::LEFT));
    assert_eq!(
        Key::new(KeyCode::Backspace, Mods::CTRL).fallback(),
        Some(Key::BACKSPACE)
    );
    assert_eq!(Key::f(5).with(Mods::ALT).fallback(), Some(Key::f(5)));
    assert_eq!(
        Key::new(KeyCode::Char('x'), Mods::CTRL | Mods::SHIFT).fallback(),
        Some(Key::ctrl('x'))
    );
    assert_eq!(Key::ctrl('x').fallback(), None);
    assert_eq!(Key::char('x').fallback(), None);
    assert_eq!(Key::LEFT.fallback(), None);
}

#[test]
fn vt100_plain_keys() {
    assert_eq!(Key::char('a').to_vt100_bytes(), b"a");
    assert_eq!(Key::char('ł').to_vt100_bytes(), "ł".as_bytes());
    assert_eq!(Key::ENTER.to_vt100_bytes(), b"\r");
    assert_eq!(Key::ESC.to_vt100_bytes(), b"\x1b");
    assert_eq!(Key::TAB.to_vt100_bytes(), b"\t");
    assert_eq!(Key::SHIFT_TAB.to_vt100_bytes(), b"\x1b[Z");
    assert_eq!(Key::BACKSPACE.to_vt100_bytes(), b"\x7f");
    assert_eq!(Key::SHIFT_SPACE.to_vt100_bytes(), b" ");
    assert_eq!(Key::UP.to_vt100_bytes(), b"\x1b[A");
    assert_eq!(Key::HOME.to_vt100_bytes(), b"\x1b[H");
    assert_eq!(Key::DELETE.to_vt100_bytes(), b"\x1b[3~");
    assert_eq!(Key::PAGE_DOWN.to_vt100_bytes(), b"\x1b[6~");
}

#[test]
fn vt100_modified_keys_use_xterm_parameters() {
    assert_eq!(Key::LEFT.with(Mods::ALT).to_vt100_bytes(), b"\x1b[1;3D");
    assert_eq!(Key::UP.with(Mods::SHIFT).to_vt100_bytes(), b"\x1b[1;2A");
    assert_eq!(Key::RIGHT.with(Mods::CTRL).to_vt100_bytes(), b"\x1b[1;5C");
    assert_eq!(
        Key::END
            .with(Mods::CTRL | Mods::SHIFT | Mods::ALT)
            .to_vt100_bytes(),
        b"\x1b[1;8F"
    );
    assert_eq!(
        Key::new(KeyCode::PageUp, Mods::CTRL).to_vt100_bytes(),
        b"\x1b[5;5~"
    );
    assert_eq!(
        Key::new(KeyCode::Delete, Mods::CTRL).to_vt100_bytes(),
        b"\x1b[3;5~"
    );
}

#[test]
fn vt100_function_keys() {
    assert_eq!(Key::f(1).to_vt100_bytes(), b"\x1bOP");
    assert_eq!(Key::f(5).to_vt100_bytes(), b"\x1b[15~");
    assert_eq!(Key::f(12).to_vt100_bytes(), b"\x1b[24~");
    assert_eq!(Key::f(5).with(Mods::SHIFT).to_vt100_bytes(), b"\x1b[15;2~");
    assert_eq!(Key::f(2).with(Mods::CTRL).to_vt100_bytes(), b"\x1b[1;5Q");
    assert!(Key::f(24).to_vt100_bytes().is_empty());
}

#[test]
fn vt100_ctrl_and_alt_characters() {
    assert_eq!(Key::ctrl('c').to_vt100_bytes(), [0x03]);
    assert_eq!(Key::ctrl(']').to_vt100_bytes(), [0x1d]);
    assert_eq!(Key::ctrl(' ').to_vt100_bytes(), [0x00]);
    assert_eq!(Key::alt('x').to_vt100_bytes(), b"\x1bx");
    assert_eq!(
        Key::new(KeyCode::Char('x'), Mods::ALT | Mods::SHIFT).to_vt100_bytes(),
        b"\x1bX"
    );
    assert_eq!(
        Key::new(KeyCode::Char('c'), Mods::CTRL | Mods::ALT).to_vt100_bytes(),
        [0x1b, 0x03]
    );
    assert_eq!(
        Key::new(KeyCode::Backspace, Mods::CTRL).to_vt100_bytes(),
        [0x08]
    );
    assert_eq!(
        Key::new(KeyCode::Backspace, Mods::ALT).to_vt100_bytes(),
        [0x1b, 0x7f]
    );
}

#[test]
fn vt100_ctrl_digits_send_nothing() {
    assert!(Key::ctrl('1').to_vt100_bytes().is_empty());
    assert!(Key::alt('1').with(Mods::CTRL).to_vt100_bytes().is_empty());
    assert_eq!(Key::alt('1').to_vt100_bytes(), b"\x1b1");
}
