use super::{contrasting_color, Color, ColorStyle};

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
fn test_contrasting_color_grayscale_ramp() {
    assert_eq!(contrasting_color(Color::Ansi256(232)), Color::White);
    assert_eq!(contrasting_color(Color::Ansi256(255)), Color::Black);
}

#[test]
fn test_contrasting_color_cube_near_white_gets_black() {
    assert_eq!(contrasting_color(Color::Ansi256(231)), Color::Black);
}

#[test]
fn test_contrasting_color_cube_black_gets_white() {
    assert_eq!(contrasting_color(Color::Ansi256(16)), Color::White);
}
