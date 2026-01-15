use super::*;
#[allow(unused_imports)]
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

#[test]
fn test_select_view_navigation() {
    use crate::key::Key;
    let mut view = SelectView::new().with_selectable(vec![true, false, false, true]); // 0 and 3 are selectable

    // Set initial
    view.set_selected_line(Some(0));

    // Move down - should skip 1, 2, land on 3
    view.handle_input(Key::ArrowDown);
    assert_eq!(view.selected_line, Some(3));

    // Move down again - should stay on 3
    view.handle_input(Key::ArrowDown);
    assert_eq!(view.selected_line, Some(3));

    // Move up - should skip 2, 1, land on 0
    view.handle_input(Key::ArrowUp);
    assert_eq!(view.selected_line, Some(0));
}

#[test]
fn test_select_view_callbacks() {
    use crate::component::EventResult;
    use crate::key::Key;
    use std::cell::RefCell;

    use std::rc::Rc;
    let selected = Rc::new(RefCell::new(None));
    let changed = Rc::new(RefCell::new(None));

    {
        let selected_clone = selected.clone();
        let changed_clone = changed.clone();
        let mut view = SelectView::new()
            .with_selectable(vec![true, true])
            .on_select(move |idx| {
                *selected_clone.borrow_mut() = Some(idx);
                EventResult::Consumed
            })
            .on_change(move |idx| {
                *changed_clone.borrow_mut() = Some(idx);
                EventResult::Consumed
            });

        view.set_selected_line(Some(0));

        // Move down -> changes 0->1
        view.handle_input(Key::ArrowDown);
        assert_eq!(*changed.borrow(), Some(1));

        // Enter -> select 1
        view.handle_input(Key::Enter);
        assert_eq!(*selected.borrow(), Some(1));
    }
}

#[test]
fn test_select_view_on_change_propagation() {
    use crate::component::EventResult;
    use crate::key::Key;

    #[derive(Debug, PartialEq, Clone)]
    struct TestAction(usize);

    let mut view = SelectView::new()
        .with_selectable(vec![true, true])
        .on_change(|idx| EventResult::Action(Box::new(TestAction(idx))));

    view.set_selected_line(Some(0));

    // Move down -> invokes on_change(1) -> returns Action(TestAction(1))
    let result = view.handle_input(Key::ArrowDown);

    match result {
        EventResult::Action(action) => {
            if let Some(test_action) = action.downcast_ref::<TestAction>() {
                assert_eq!(test_action.0, 1);
            } else {
                panic!("Action was not TestAction");
            }
        }
        _ => panic!("Expected Action, got {:?}", result),
    }
}

#[test]
fn test_select_view_scrolling() {
    use crate::key::Key;
    use crate::layer::Cell;
    use crate::layer::{Layer, LayerPriority};

    // Create a small layer: 10 rows.
    // SelectView uses 90% height. 10 * 0.9 = 9.
    // Subtract 2 for borders = 7 content height.
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 10, 80);
    let mut view = SelectView::new();

    // Add 10 lines (more than 7)
    let content: Vec<Vec<Cell>> = (0..10)
        .map(|i| format!("Line {}", i).chars().map(Cell::from_char).collect())
        .collect();
    view.set_left_content(content);

    // Initial state
    view.set_selected_line(Some(0));
    assert_eq!(view.left_scroll, 0);

    // Render to calculate height
    view.render(&mut layer);

    // Move down 6 times (0 -> 6). Should be visible.
    for _ in 0..6 {
        view.handle_input(Key::ArrowDown);
    }
    assert_eq!(view.selected_line, Some(6));
    assert_eq!(view.left_scroll, 0);

    // Move down to 7. Should scroll.
    view.handle_input(Key::ArrowDown); // selected 7
    assert_eq!(view.selected_line, Some(7));
    assert_eq!(view.left_scroll, 1);

    // Move up to 0. Should scroll back.
    for _ in 0..7 {
        view.handle_input(Key::ArrowUp);
    }
    assert_eq!(view.selected_line, Some(0));
    assert_eq!(view.left_scroll, 0);
}

#[test]
fn test_select_view_initial_scroll() {
    use crate::layer::Cell;
    use crate::layer::{Layer, LayerPriority};

    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 10, 80);
    let mut view = SelectView::new();

    // Add 20 lines
    let content: Vec<Vec<Cell>> = (0..20)
        .map(|i| format!("Line {}", i).chars().map(Cell::from_char).collect())
        .collect();
    view.set_left_content(content);

    // Initial state: select line 14.
    // Viewport height is 7 (10*0.9 - 2).
    // Visible range would be [0, 7) if scroll is 0.
    // 14 is out of bounds.
    // Should scroll to make 14 visible at bottom?
    // 14 - 7 + 1 = 8. Range [8, 15). 14 is inside.
    view.set_selected_line(Some(14));

    // Render to trigger auto-scroll
    view.render(&mut layer);

    // Check scroll
    assert!(view.left_scroll > 0);
    assert_eq!(view.left_scroll, 8); // 14 - 7 + 1

    // Case 2: scroll too far down, select top
    view.set_left_scroll(10);
    view.set_selected_line(Some(2));
    view.render(&mut layer);
    assert_eq!(view.left_scroll, 2);
}
