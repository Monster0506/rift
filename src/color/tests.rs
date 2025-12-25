//! Tests for color system

use super::{Color, ColorStyle};
use crate::color::styled::{StyledChar, ColorSpan, StyledLine};
use crate::color::buffer::ColorMap;

#[test]
fn test_color_basic() {
    let red = Color::Red;
    let crossterm_color = red.to_crossterm();
    let back = Color::from_crossterm(crossterm_color);
    assert_eq!(red, back);
}

#[test]
fn test_color_rgb() {
    let rgb = Color::Rgb { r: 255, g: 128, b: 64 };
    let crossterm_color = rgb.to_crossterm();
    let back = Color::from_crossterm(crossterm_color);
    assert_eq!(rgb, back);
}

#[test]
fn test_color_ansi256() {
    let ansi = Color::Ansi256(100);
    let crossterm_color = ansi.to_crossterm();
    let back = Color::from_crossterm(crossterm_color);
    assert_eq!(ansi, back);
}

#[test]
fn test_color_style() {
    let style = ColorStyle::new();
    assert!(style.is_empty());

    let style = ColorStyle::fg(Color::Red);
    assert!(!style.is_empty());
    assert_eq!(style.fg, Some(Color::Red));
    assert_eq!(style.bg, None);

    let style = ColorStyle::bg(Color::Blue);
    assert_eq!(style.fg, None);
    assert_eq!(style.bg, Some(Color::Blue));

    let style = ColorStyle::new_colors(Color::Red, Color::Blue);
    assert_eq!(style.fg, Some(Color::Red));
    assert_eq!(style.bg, Some(Color::Blue));
}

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
}

#[test]
fn test_color_map() {
    let mut map = ColorMap::new();
    map.set_char(0, 0, ColorStyle::fg(Color::Red));
    
    let style = map.get_style(0, 0);
    assert_eq!(style, Some(ColorStyle::fg(Color::Red)));
    
    let style = map.get_style(0, 1);
    assert_eq!(style, None);
}

#[test]
fn test_color_map_span() {
    let mut map = ColorMap::new();
    map.set_span(0, 0, 5, ColorStyle::fg(Color::Blue));
    
    for i in 0..5 {
        assert_eq!(map.get_style(0, i), Some(ColorStyle::fg(Color::Blue)));
    }
    assert_eq!(map.get_style(0, 5), None);
}

