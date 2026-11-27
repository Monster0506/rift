use super::*;
use crate::layer::Layer;

fn row_text(layer: &Layer, row: usize) -> String {
    (0..layer.cols())
        .map(|c| match layer.get_cell(row, c).map(|c| c.content) {
            Some(crate::character::Character::Unicode(ch)) => ch,
            _ => '.',
        })
        .collect()
}

fn drawn_rows(layer: &Layer) -> Vec<usize> {
    (0..layer.rows())
        .filter(|&r| (0..layer.cols()).any(|c| layer.get_cell(r, c).is_some()))
        .collect()
}

fn popup_with(content: PopupContent) -> Popup {
    let mut popup = Popup::default();
    popup.open(content);
    popup
}

#[test]
fn anchored_float_sits_below_cursor_row_when_it_fits() {
    let mut layer = Layer::new(20, 40);
    let lines = vec!["one".to_string(), "two".to_string()];
    let mut popup = popup_with(PopupContent::new("T", lines).with_anchor_row(3));
    popup.render_into_layer(&mut layer, None, None);
    assert_eq!(drawn_rows(&layer), vec![4, 5, 6, 7]);
    assert!(
        row_text(&layer, 4).contains(" T "),
        "title sits on the top border"
    );
    assert!(row_text(&layer, 5).contains("one"));
}

#[test]
fn anchored_float_moves_above_cursor_row_when_no_room_below() {
    let mut layer = Layer::new(20, 40);
    let lines = vec!["one".to_string(), "two".to_string()];
    let mut popup = popup_with(PopupContent::new("T", lines).with_anchor_row(17));
    popup.render_into_layer(&mut layer, None, None);
    assert_eq!(drawn_rows(&layer), vec![13, 14, 15, 16]);
}

#[test]
fn anchored_float_clamps_and_scrolls_long_content() {
    let mut layer = Layer::new(12, 40);
    let lines: Vec<String> = (0..20).map(|i| format!("line{i}")).collect();
    let mut popup = popup_with(PopupContent::new("T", lines).with_anchor_row(2));
    popup.render_into_layer(&mut layer, None, None);
    assert_eq!(drawn_rows(&layer), (3..=10).collect::<Vec<_>>());
    assert!(row_text(&layer, 3).contains("T [6/20]"));
    assert!(row_text(&layer, 4).contains("line0"));

    popup.scroll(100);
    popup.render_into_layer(&mut layer, None, None);
    assert!(row_text(&layer, 3).contains("T [20/20]"));
    assert!(row_text(&layer, 4).contains("line14"));

    popup.scroll(-3);
    popup.render_into_layer(&mut layer, None, None);
    assert!(row_text(&layer, 3).contains("T [17/20]"));
    assert!(row_text(&layer, 4).contains("line11"));
}

#[test]
fn unanchored_float_stays_centered() {
    let mut layer = Layer::new(20, 40);
    let lines = vec!["one".to_string()];
    let mut popup = popup_with(PopupContent::new("T", lines));
    popup.render_into_layer(&mut layer, None, None);
    assert_eq!(drawn_rows(&layer), vec![8, 9, 10]);
}
