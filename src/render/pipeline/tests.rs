use super::*;
use crate::character::Character;
use crate::render::Color;
use crate::search::SearchMatch;

// Helper to create basic items
fn chars(s: &str) -> Vec<RenderItem> {
    let mut offset = 0;
    s.chars()
        .map(|c| {
            let len = c.len_utf8();
            let item = RenderItem::new(Character::from(c), offset, len);
            offset += len;
            item
        })
        .collect()
}

#[test]
fn test_tab_layout() {
    // "a\tb" with tab_width 4
    // 'a' (width 1, col 1)
    // '\t' (width 3 -> 4, col 4)
    // 'b' (width 1, col 5)
    let input = chars("a\tb").into_iter();
    let layout = TabLayout::new(input, 4);
    let items: Vec<LayoutItem> = layout.collect();

    assert_eq!(items.len(), 3);
    assert_eq!(items[0].char, Character::from('a'));
    assert_eq!(items[0].width, 1);

    assert_eq!(items[1].char, Character::from('\t'));
    assert_eq!(items[1].width, 3); // 4 - (1 % 4) = 3

    assert_eq!(items[2].char, Character::from('b'));
    assert_eq!(items[2].width, 1);
}

#[test]
fn test_search_decorator() {
    let input = chars("hello world").into_iter();
    // Match "world" (offset 6..11)
    let matches = vec![SearchMatch { range: 6..11 }];
    let mut matches_idx = 0;

    let decorator = SearchDecorator::new(input, &matches, &mut matches_idx);
    let items: Vec<RenderItem> = decorator.collect();

    // Check "hello "
    for i in 0..6 {
        assert!(items[i].fg.is_none());
        assert!(items[i].bg.is_none());
    }

    // Check "world"
    for i in 6..11 {
        assert_eq!(items[i].fg, Some(Color::Black));
        assert_eq!(items[i].bg, Some(Color::Yellow));
    }
}

#[test]
fn test_syntax_decorator() {
    let input = chars("fn main()").into_iter();

    let highlights = vec![];
    let mut idx = 0;
    let decorator = SyntaxDecorator::new(input, &highlights, &mut idx, None, None);
    let items: Vec<RenderItem> = decorator.collect();

    assert_eq!(items.len(), 9);
    assert_eq!(items[0].char, Character::from('f'));
}
// ColorDecorator tests

#[test]
fn test_color_decorator_no_highlights_no_color() {
    let input = chars("hello").into_iter();
    let highlights: Vec<(std::ops::Range<usize>, Color)> = vec![];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    assert_eq!(items.len(), 5);
    for item in &items {
        assert!(item.fg.is_none(), "no highlights → no color");
    }
}

#[test]
fn test_color_decorator_full_range_colored() {
    let input = chars("abc").into_iter();
    let highlights = vec![(0..3, Color::Red)];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    assert_eq!(items.len(), 3);
    for item in &items {
        assert_eq!(item.fg, Some(Color::Red));
    }
}

#[test]
fn test_color_decorator_partial_range_colors_only_matching() {
    let input = chars("abcde").into_iter();
    // Only "bc" (offsets 1..3) should be colored
    let highlights = vec![(1..3, Color::Blue)];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    assert_eq!(items.len(), 5);
    assert!(items[0].fg.is_none(), "'a' before range");
    assert_eq!(items[1].fg, Some(Color::Blue), "'b' in range");
    assert_eq!(items[2].fg, Some(Color::Blue), "'c' in range");
    assert!(items[3].fg.is_none(), "'d' after range");
    assert!(items[4].fg.is_none(), "'e' after range");
}

#[test]
fn test_color_decorator_multiple_ranges() {
    let input = chars("abcde").into_iter();
    let highlights = vec![
        (0..2, Color::Red),   // "ab" → red
        (3..5, Color::Green), // "de" → green
    ];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    assert_eq!(items[0].fg, Some(Color::Red));
    assert_eq!(items[1].fg, Some(Color::Red));
    assert!(items[2].fg.is_none(), "'c' between ranges");
    assert_eq!(items[3].fg, Some(Color::Green));
    assert_eq!(items[4].fg, Some(Color::Green));
}

#[test]
fn test_color_decorator_advances_past_expired_range() {
    // Range 0..2 ends before offset 3. The decorator should skip it and apply range 3..5.
    let input = chars("abcde").into_iter();
    let highlights = vec![
        (0..2, Color::Red),
        (3..5, Color::Yellow),
    ];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    assert_eq!(items[3].fg, Some(Color::Yellow));
    assert_eq!(items[4].fg, Some(Color::Yellow));
}

#[test]
fn test_color_decorator_range_starts_after_all_items() {
    let input = chars("ab").into_iter();
    // Range starts at offset 10, beyond any item
    let highlights = vec![(10..12, Color::Cyan)];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    for item in &items {
        assert!(item.fg.is_none(), "range beyond input → no color applied");
    }
}

#[test]
fn test_color_decorator_single_char_range() {
    let input = chars("xyz").into_iter();
    // Only 'y' at offset 1
    let highlights = vec![(1..2, Color::Magenta)];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    assert!(items[0].fg.is_none());
    assert_eq!(items[1].fg, Some(Color::Magenta));
    assert!(items[2].fg.is_none());
}

#[test]
fn test_color_decorator_overwrites_existing_fg() {
    // Start with pre-colored items
    let mut offset = 0usize;
    let pre_colored: Vec<RenderItem> = "ab".chars().map(|c| {
        let len = c.len_utf8();
        let mut item = RenderItem::new(Character::from(c), offset, len);
        item.fg = Some(Color::White);
        offset += len;
        item
    }).collect();
    let highlights = vec![(0..2, Color::Red)];
    let items: Vec<RenderItem> = ColorDecorator::new(pre_colored.into_iter(), &highlights).collect();
    // ColorDecorator should overwrite with Red
    assert_eq!(items[0].fg, Some(Color::Red));
    assert_eq!(items[1].fg, Some(Color::Red));
}

#[test]
fn test_color_decorator_empty_input() {
    let input: Vec<RenderItem> = vec![];
    let highlights = vec![(0..5, Color::Blue)];
    let items: Vec<RenderItem> = ColorDecorator::new(input.into_iter(), &highlights).collect();
    assert!(items.is_empty());
}

#[test]
fn test_color_decorator_adjacent_ranges_no_gap() {
    let input = chars("abcd").into_iter();
    let highlights = vec![
        (0..2, Color::Red),
        (2..4, Color::Blue),
    ];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    assert_eq!(items[0].fg, Some(Color::Red));
    assert_eq!(items[1].fg, Some(Color::Red));
    assert_eq!(items[2].fg, Some(Color::Blue));
    assert_eq!(items[3].fg, Some(Color::Blue));
}

#[test]
fn test_color_decorator_preserves_byte_offset() {
    let input = chars("hello").into_iter();
    let highlights = vec![(0..5, Color::Green)];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    for (i, item) in items.iter().enumerate() {
        assert_eq!(item.byte_offset, i, "byte_offset must not change");
    }
}

#[test]
fn test_color_decorator_preserves_char_values() {
    let input = chars("rust").into_iter();
    let highlights = vec![(0..4, Color::Yellow)];
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights).collect();
    let result: String = items.iter().map(|i| {
        match i.char {
            Character::Unicode(c) => c,
            Character::Tab => '\t',
            Character::Newline => '\n',
            _ => '?',
        }
    }).collect();
    assert_eq!(result, "rust");
}
