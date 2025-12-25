//! Tests for color system

use super::{Color, ColorStyle};


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

