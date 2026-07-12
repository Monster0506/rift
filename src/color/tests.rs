//! Tests for color system

use super::{Color, ColorStyle};

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
