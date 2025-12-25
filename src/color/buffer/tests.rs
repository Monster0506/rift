use super::*;
use crate::color::Color;

#[test]
fn test_color_map_set_get() {
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

#[test]
fn test_color_map_merge() {
    let mut map = ColorMap::new();
    map.set_span(0, 0, 3, ColorStyle::fg(Color::Red));
    map.set_span(0, 3, 6, ColorStyle::fg(Color::Red)); // Adjacent with same style
    
    let spans = map.get_line_spans(0);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, 6);
}

#[test]
fn test_color_map_clear() {
    let mut map = ColorMap::new();
    map.set_char(0, 0, ColorStyle::fg(Color::Red));
    map.set_char(1, 0, ColorStyle::fg(Color::Blue));
    
    map.clear_line(0);
    assert_eq!(map.get_style(0, 0), None);
    assert_eq!(map.get_style(1, 0), Some(ColorStyle::fg(Color::Blue)));
    
    map.clear();
    assert!(map.is_empty());
}
