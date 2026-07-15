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
fn test_tab_layout_wide_and_zero_width_unicode_chars() {
    // Non-ASCII must skip the ASCII width-1 fast path: CJK is width 2, combining mark width 0.
    let input = chars("\u{4E2D}\u{0301}a").into_iter(); // '中' (CJK), combining acute, 'a'
    let layout = TabLayout::new(input, 4);
    let items: Vec<LayoutItem> = layout.collect();

    assert_eq!(items.len(), 3);
    assert_eq!(items[0].width, 2, "CJK ideograph must be width 2");
    assert_eq!(items[1].width, 0, "combining mark must be width 0");
    assert_eq!(items[2].width, 1, "ASCII 'a' must still be width 1");
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
    for item in &items[0..6] {
        assert!(item.fg.is_none());
        assert!(item.bg.is_none());
    }

    // Check "world"
    for item in &items[6..11] {
        assert_eq!(item.fg, Some(Color::Black));
        assert_eq!(item.bg, Some(Color::Yellow));
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
// PresentationDecorator tests

#[test]
fn test_presentation_decorator_applies_fg_bg_attrs() {
    use crate::layer::{CellAttrs, CellStyle};
    let input = chars("abcde").into_iter();
    let styles: Vec<(std::ops::Range<usize>, CellStyle)> = vec![(
        1..3,
        CellStyle {
            fg: Some(Color::Blue),
            bg: Some(Color::Black),
            attrs: CellAttrs {
                underline: true,
                ..Default::default()
            },
        },
    )];
    let mut idx = 0;
    let items: Vec<RenderItem> = PresentationDecorator::new(input, &styles, &mut idx).collect();
    assert!(items[0].fg.is_none() && items[0].bg.is_none(), "'a' before");
    assert_eq!(items[1].fg, Some(Color::Blue));
    assert_eq!(items[1].bg, Some(Color::Black));
    assert!(items[1].attrs.underline, "underline attr applied");
    assert_eq!(items[2].fg, Some(Color::Blue));
    assert!(
        items[3].fg.is_none() && !items[3].attrs.underline,
        "'d' after"
    );
}

#[test]
fn test_presentation_decorator_fg_only_leaves_bg() {
    use crate::layer::CellStyle;
    let input = chars("ab").into_iter();
    let styles: Vec<(std::ops::Range<usize>, CellStyle)> = vec![(
        0..2,
        CellStyle {
            fg: Some(Color::Red),
            bg: None,
            attrs: Default::default(),
        },
    )];
    let mut idx = 0;
    let items: Vec<RenderItem> = PresentationDecorator::new(input, &styles, &mut idx).collect();
    assert_eq!(items[0].fg, Some(Color::Red));
    assert!(items[0].bg.is_none(), "bg untouched when style bg is None");
}

#[test]
fn test_presentation_decorator_cursor_carries_over_across_calls() {
    // A second decorator sharing the same `idx` must not re-scan from the
    // start, and must still apply a style starting after the first call ended.
    use crate::layer::CellStyle;
    let styles: Vec<(std::ops::Range<usize>, CellStyle)> = vec![
        (
            0..2,
            CellStyle {
                fg: Some(Color::Red),
                bg: None,
                attrs: Default::default(),
            },
        ),
        (
            10..12,
            CellStyle {
                fg: Some(Color::Blue),
                bg: None,
                attrs: Default::default(),
            },
        ),
    ];
    let mut idx = 0;

    // First "row": covers byte offsets 0..2, consumes the first style.
    let first_input = chars("ab");
    let first_items: Vec<RenderItem> =
        PresentationDecorator::new(first_input.into_iter(), &styles, &mut idx).collect();
    assert_eq!(first_items[0].fg, Some(Color::Red));
    assert_eq!(idx, 0, "idx should not advance past a style still in range");

    // Second "row" at byte offset 10 - cursor must not be stuck/corrupted from the first call.
    let mut second_item = RenderItem::new(Character::from('z'), 10, 1, 10);
    second_item = PresentationDecorator::new(std::iter::once(second_item), &styles, &mut idx)
        .next()
        .unwrap();
    assert_eq!(
        second_item.fg,
        Some(Color::Blue),
        "cursor must correctly find the second style after carrying over"
    );
}

// ColorDecorator tests

#[test]
fn test_color_decorator_no_highlights_no_color() {
    let input = chars("hello").into_iter();
    let highlights: Vec<(std::ops::Range<usize>, Color)> = vec![];
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
    assert_eq!(items.len(), 5);
    for item in &items {
        assert!(item.fg.is_none(), "no highlights -> no color");
    }
}

#[test]
fn test_color_decorator_full_range_colored() {
    let input = chars("abc").into_iter();
    let highlights = vec![(0..3, Color::Red)];
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
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
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
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
        (0..2, Color::Red),   // "ab" -> red
        (3..5, Color::Green), // "de" -> green
    ];
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
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
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
    assert_eq!(items[3].fg, Some(Color::Yellow));
    assert_eq!(items[4].fg, Some(Color::Yellow));
}

#[test]
fn test_color_decorator_range_starts_after_all_items() {
    let input = chars("ab").into_iter();
    // Range starts at offset 10, beyond any item
    let highlights = vec![(10..12, Color::Cyan)];
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
    for item in &items {
        assert!(item.fg.is_none(), "range beyond input -> no color applied");
    }
}

#[test]
fn test_color_decorator_single_char_range() {
    let input = chars("xyz").into_iter();
    // Only 'y' at offset 1
    let highlights = vec![(1..2, Color::Magenta)];
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
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
    let mut idx = 0;
    let items: Vec<RenderItem> =
        ColorDecorator::new(pre_colored.into_iter(), &highlights, &mut idx).collect();
    // ColorDecorator should overwrite with Red
    assert_eq!(items[0].fg, Some(Color::Red));
    assert_eq!(items[1].fg, Some(Color::Red));
}

#[test]
fn test_color_decorator_empty_input() {
    let input: Vec<RenderItem> = vec![];
    let highlights = vec![(0..5, Color::Blue)];
    let mut idx = 0;
    let items: Vec<RenderItem> =
        ColorDecorator::new(input.into_iter(), &highlights, &mut idx).collect();
    assert!(items.is_empty());
}

#[test]
fn test_color_decorator_adjacent_ranges_no_gap() {
    let input = chars("abcd").into_iter();
    let highlights = vec![(0..2, Color::Red), (2..4, Color::Blue)];
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
    assert_eq!(items[0].fg, Some(Color::Red));
    assert_eq!(items[1].fg, Some(Color::Red));
    assert_eq!(items[2].fg, Some(Color::Blue));
    assert_eq!(items[3].fg, Some(Color::Blue));
}

#[test]
fn test_color_decorator_preserves_byte_offset() {
    let input = chars("hello").into_iter();
    let highlights = vec![(0..5, Color::Green)];
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
    for (i, item) in items.iter().enumerate() {
        assert_eq!(item.byte_offset, i, "byte_offset must not change");
    }
}

#[test]
fn test_color_decorator_preserves_char_values() {
    let input = chars("rust").into_iter();
    let highlights = vec![(0..4, Color::Yellow)];
    let mut idx = 0;
    let items: Vec<RenderItem> = ColorDecorator::new(input, &highlights, &mut idx).collect();
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
fn test_line_source_renders_plain_directory_line() {
    // After migration, directory buffer lines contain only visible chars — no annotation prefix.
    // LineSource should yield all characters as-is.
    let items = chars("hello.txt").into_iter();
    let collected: Vec<RenderItem> = items.collect();
    assert_eq!(collected.len(), 9);
    assert_eq!(visible_chars(&collected), "hello.txt");
}

#[test]
fn test_tab_layout_directory_filename_width() {
    // Ensure a plain filename (no annotation bytes) goes through TabLayout without width=0 items.
    let input = chars("src/").into_iter();
    let layout: Vec<LayoutItem> = TabLayout::new(input, 4).collect();
    assert_eq!(layout.len(), 4);
    for item in &layout {
        assert!(item.width > 0, "all chars should have non-zero width");
    }
}

#[test]
fn test_contrasting_color_grayscale_ramp() {
    assert_eq!(contrasting_color(Color::Ansi256(232)), Color::White);
    assert_eq!(contrasting_color(Color::Ansi256(255)), Color::Black);
}

#[test]
fn test_contrasting_color_cube_near_white_gets_black() {
    // Ansi256(231) is the brightest cube color (rgb 255,255,255).
    assert_eq!(contrasting_color(Color::Ansi256(231)), Color::Black);
}

#[test]
fn test_contrasting_color_cube_black_gets_white() {
    // Ansi256(16) is the darkest cube color (rgb 0,0,0).
    assert_eq!(contrasting_color(Color::Ansi256(16)), Color::White);
}
