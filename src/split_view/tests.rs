use super::*;
use crate::layer::Layer;

#[test]
fn test_split_view_new() {
    let view = SplitView::new();
    assert_eq!(view.left_width_percent, 40);
    assert!(view.left_content.is_empty());
    assert!(view.right_content.is_empty());
}

#[test]
fn test_split_view_with_left_width() {
    let view = SplitView::new().with_left_width(60);
    assert_eq!(view.left_width_percent, 60);

    // Clamped to 90 max
    let view = SplitView::new().with_left_width(95);
    assert_eq!(view.left_width_percent, 90);

    // Clamped to 10 min
    let view = SplitView::new().with_left_width(5);
    assert_eq!(view.left_width_percent, 10);
}

#[test]
fn test_split_view_set_content() {
    let mut view = SplitView::new();
    view.set_left_content(vec!["left".chars().collect()]);
    view.set_right_content(vec!["right".chars().collect()]);

    assert_eq!(view.left_content.len(), 1);
    assert_eq!(view.right_content.len(), 1);
}

#[test]
fn test_split_view_render() {
    use crate::layer::LayerPriority;
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let mut view = SplitView::new();

    view.set_left_content(vec![
        "Line 1 left".chars().collect(),
        "Line 2 left".chars().collect(),
    ]);
    view.set_right_content(vec![
        "Line 1 right".chars().collect(),
        "Line 2 right".chars().collect(),
    ]);

    // Should not panic
    view.render(&mut layer);
}
