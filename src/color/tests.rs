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

#[test]
fn color_names_parse_back_to_the_same_color() {
    for color in [
        Color::Reset,
        Color::DarkYellow,
        Color::Grey,
        Color::Ansi256(208),
        Color::Rgb {
            r: 1,
            g: 171,
            b: 255,
        },
    ] {
        assert_eq!(Color::parse(&color.name()), Some(color), "{}", color.name());
    }
}
