use super::*;
use crate::layer::Layer;

#[test]
fn test_select_view_new() {
    let view = SelectView::new();
    assert_eq!(view.left_width_percent, 40);
    assert!(view.left_content.is_empty());
    assert!(view.right_content.is_empty());
}

#[test]
fn test_select_view_with_left_width() {
    let view = SelectView::new().with_left_width(60);
    assert_eq!(view.left_width_percent, 60);

    // Clamped to 90 max
    let view = SelectView::new().with_left_width(95);
    assert_eq!(view.left_width_percent, 90);

    // Clamped to 10 min
    let view = SelectView::new().with_left_width(5);
    assert_eq!(view.left_width_percent, 10);
}

#[test]
fn test_select_view_set_content() {
    use crate::layer::Cell;
    let mut view = SelectView::new();
    let left: Vec<Cell> = "left".chars().map(Cell::from_char).collect();
    let right: Vec<Cell> = "right".chars().map(Cell::from_char).collect();
    view.set_left_content(vec![left]);
    view.set_right_content(vec![right]);

    assert_eq!(view.left_content.len(), 1);
    assert_eq!(view.right_content.len(), 1);
}

#[test]
fn test_select_view_render() {
    use crate::layer::LayerPriority;
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let mut view = SelectView::new();

    use crate::layer::Cell;
    view.set_left_content(vec![
        "Line 1 left".chars().map(Cell::from_char).collect(),
        "Line 2 left".chars().map(Cell::from_char).collect(),
    ]);
    view.set_right_content(vec![
        "Line 1 right".chars().map(Cell::from_char).collect(),
        "Line 2 right".chars().map(Cell::from_char).collect(),
    ]);

    // Should not panic
    view.render(&mut layer);
}
