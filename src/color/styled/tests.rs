use super::*;
use crate::color::Color;

#[test]
fn test_styled_char() {
    let sc = StyledChar::new(b'a', ColorStyle::fg(Color::Red));
    assert_eq!(sc.ch, b'a');
    assert_eq!(sc.style.fg, Some(Color::Red));
}

#[test]
fn test_color_span() {
    let span = ColorSpan::new(0, 5, ColorStyle::fg(Color::Blue));
    assert_eq!(span.start, 0);
    assert_eq!(span.end, 5);
    assert_eq!(span.len(), 5);
    assert!(!span.is_empty());

    let empty = ColorSpan::new(5, 5, ColorStyle::new());
    assert!(empty.is_empty());
}

#[test]
fn test_styled_line_plain() {
    let line = StyledLine::plain(b"hello".to_vec());
    assert_eq!(line.len(), 5);
    assert_eq!(line.as_bytes(), b"hello".to_vec());
}

#[test]
fn test_styled_line_per_char() {
    let chars = vec![
        StyledChar::new(b'h', ColorStyle::fg(Color::Red)),
        StyledChar::new(b'e', ColorStyle::fg(Color::Blue)),
    ];
    let line = StyledLine::per_char(chars.clone());
    assert_eq!(line.len(), 2);
    assert_eq!(line.as_bytes(), b"he".to_vec());
}

#[test]
fn test_styled_line_per_span() {
    let text = b"hello world".to_vec();
    let spans = vec![
        ColorSpan::new(0, 5, ColorStyle::fg(Color::Red)),
        ColorSpan::new(6, 11, ColorStyle::fg(Color::Blue)),
    ];
    let line = StyledLine::per_span(text.clone(), spans);
    assert_eq!(line.len(), 11);
    assert_eq!(line.get_style_at(0).fg, Some(Color::Red));
    assert_eq!(line.get_style_at(6).fg, Some(Color::Blue));
    assert_eq!(line.get_style_at(5).fg, None); // Space has no color
}

#[test]
fn test_styled_line_conversion() {
    let text = b"hello".to_vec();
    let spans = vec![
        ColorSpan::new(0, 5, ColorStyle::fg(Color::Red)),
    ];
    let line = StyledLine::per_span(text, spans);
    let per_char = line.to_per_char();
    assert_eq!(per_char.len(), 5);
    assert_eq!(per_char[0].style.fg, Some(Color::Red));
}
