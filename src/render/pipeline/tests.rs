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
