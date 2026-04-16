use super::*;
use crate::character::Character;
use crate::render::Color;
use crate::search::SearchMatch;

// Helper to create basic items
fn chars(s: &str) -> Vec<RenderItem> {
    let mut byte_offset = 0;
    let mut char_offset = 0;
    s.chars()
        .map(|c| {
            let len = c.len_utf8();
            let item = RenderItem::new(Character::from(c), byte_offset, len, char_offset);
            byte_offset += len;
            char_offset += 1;
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
fn test_search_decorator_multibyte_unicode() {
    // "日本語" — each char is 3 bytes in UTF-8
    // char offsets: 日=0, 本=1, 語=2
    // byte offsets: 日=0, 本=3, 語=6
    // Match "語" at char offset 2..3 (byte offset 6..9)
    let input = chars("日本語").into_iter();
    let matches = vec![SearchMatch { range: 2..3 }];
    let mut matches_idx = 0;

    let decorator = SearchDecorator::new(input, &matches, &mut matches_idx);
    let items: Vec<RenderItem> = decorator.collect();

    assert_eq!(items.len(), 3);
    // 日 and 本 should NOT be highlighted
    assert!(items[0].bg.is_none(), "日 should not be highlighted");
    assert!(items[1].bg.is_none(), "本 should not be highlighted");
    // 語 should be highlighted
    assert_eq!(items[2].fg, Some(Color::Black), "語 fg");
    assert_eq!(items[2].bg, Some(Color::Yellow), "語 bg");
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
    let highlights = vec![(0..2, Color::Red), (3..5, Color::Yellow)];
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
    let mut byte_offset = 0usize;
    let mut char_offset = 0usize;
    let pre_colored: Vec<RenderItem> = "ab"
        .chars()
        .map(|c| {
            let len = c.len_utf8();
            let mut item = RenderItem::new(Character::from(c), byte_offset, len, char_offset);
            item.fg = Some(Color::White);
            byte_offset += len;
            char_offset += 1;
            item
        })
        .collect();
    let highlights = vec![(0..2, Color::Red)];
    let items: Vec<RenderItem> =
        ColorDecorator::new(pre_colored.into_iter(), &highlights).collect();
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
    let highlights = vec![(0..2, Color::Red), (2..4, Color::Blue)];
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
    let result: String = items
        .iter()
        .map(|i| match i.char {
            Character::Unicode(c) => c,
            Character::Tab => '\t',
            Character::Newline => '\n',
            _ => '?',
        })
        .collect();
    assert_eq!(result, "rust");
}

fn visible_chars(items: &[RenderItem]) -> String {
    items
        .iter()
        .map(|i| match i.char {
            Character::Unicode(c) => c,
            Character::Tab => '\t',
            Character::Newline => '\n',
            _ => '?',
        })
        .collect()
}

#[test]
fn test_invisible_filter_no_ranges_passes_all() {
    let input = chars("hello").into_iter();
    let items: Vec<RenderItem> = InvisibleFilter::new(input, &[]).collect();
    assert_eq!(items.len(), 5);
    assert_eq!(visible_chars(&items), "hello");
}

#[test]
fn test_invisible_filter_removes_prefix() {
    // Simulate /001 prefix (5 bytes: '/', '0', '0', '1', ' ') followed by "file.txt"
    let input = chars("/001 file.txt").into_iter();
    let ranges = vec![0..5usize];
    let items: Vec<RenderItem> = InvisibleFilter::new(input, &ranges).collect();
    assert_eq!(visible_chars(&items), "file.txt");
}

#[test]
fn test_invisible_filter_range_in_middle_removed() {
    // "ab[cd]ef" — bytes 2..4 invisible
    let input = chars("abcdef").into_iter();
    let ranges = vec![2..4usize];
    let items: Vec<RenderItem> = InvisibleFilter::new(input, &ranges).collect();
    assert_eq!(visible_chars(&items), "abef");
}

#[test]
fn test_invisible_filter_multiple_ranges() {
    // "/001 a /002 b" — two prefixes at offsets 0..5 and 7..12
    let input = chars("/001 a /002 b").into_iter();
    let ranges = vec![0..5usize, 7..12];
    let items: Vec<RenderItem> = InvisibleFilter::new(input, &ranges).collect();
    assert_eq!(visible_chars(&items), "a b");
}

#[test]
fn test_invisible_filter_all_hidden_yields_empty() {
    let input = chars("abc").into_iter();
    let ranges = vec![0..3usize];
    let items: Vec<RenderItem> = InvisibleFilter::new(input, &ranges).collect();
    assert!(items.is_empty());
}

#[test]
fn test_invisible_filter_range_beyond_input_harmless() {
    let input = chars("hi").into_iter();
    let ranges = vec![10..20usize]; // entirely past the input
    let items: Vec<RenderItem> = InvisibleFilter::new(input, &ranges).collect();
    assert_eq!(visible_chars(&items), "hi");
}

#[test]
fn test_invisible_filter_preserves_byte_offsets_of_visible_items() {
    // Input: "/001 abc" — prefix 0..5 invisible
    // 'a' is at byte 5, 'b' at 6, 'c' at 7
    let input = chars("/001 abc").into_iter();
    let ranges = vec![0..5usize];
    let items: Vec<RenderItem> = InvisibleFilter::new(input, &ranges).collect();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].byte_offset, 5);
    assert_eq!(items[1].byte_offset, 6);
    assert_eq!(items[2].byte_offset, 7);
}
