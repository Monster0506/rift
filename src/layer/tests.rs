//! Tests for the layer module

use super::*;
use crate::character::Character;

#[test]
fn test_layer_priority_ordering() {
    assert!(LayerPriority::CONTENT < LayerPriority::STATUS_BAR);
    assert!(LayerPriority::STATUS_BAR < LayerPriority::FLOATING_WINDOW);
    assert!(LayerPriority::FLOATING_WINDOW < LayerPriority::POPUP);
    assert!(LayerPriority::POPUP < LayerPriority::HOVER);
    assert!(LayerPriority::HOVER < LayerPriority::TOOLTIP);
}

#[test]
fn test_cell_creation() {
    let cell = Cell::from_char('A');
    assert_eq!(cell.content, Character::from('A'));
    assert_eq!(cell.fg, None);
    assert_eq!(cell.bg, None);
}

#[test]
fn test_cell_with_colors() {
    let cell = Cell::from_char('X')
        .with_fg(Color::Red)
        .with_bg(Color::Blue);
    assert_eq!(cell.content, Character::from('X'));
    assert_eq!(cell.fg, Some(Color::Red));
    assert_eq!(cell.bg, Some(Color::Blue));
}

#[test]
fn test_cell_empty() {
    let cell = Cell::empty();
    assert_eq!(cell.content, Character::from(' '));
}

#[test]
fn test_cell_from_char() {
    let cell = Cell::from_char('│');
    assert_eq!(cell.content, Character::from('│'));
}

#[test]
fn test_layer_creation() {
    let layer = Layer::new(LayerPriority::CONTENT, 10, 20);
    assert_eq!(layer.priority(), LayerPriority::CONTENT);
    assert_eq!(layer.rows(), 10);
    assert_eq!(layer.cols(), 20);
    assert!(layer.is_dirty());
}

#[test]
fn test_layer_set_and_get_cell() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 5, 5);

    // Set a cell
    assert!(layer.set_cell(2, 3, Cell::from_char('X')));

    // Get the cell back
    let cell = layer.get_cell(2, 3);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, Character::from('X'));

    // Out of bounds should return None
    assert!(layer.get_cell(10, 10).is_none());
}

#[test]
fn test_layer_out_of_bounds() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 5, 5);

    // Out of bounds set should return false
    assert!(!layer.set_cell(10, 10, Cell::from_char('X')));
    assert!(!layer.set_cell(5, 3, Cell::from_char('X')));
    assert!(!layer.set_cell(3, 5, Cell::from_char('X')));
}

#[test]
fn test_layer_clear() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 3, 3);
    layer.set_cell(1, 1, Cell::from_char('X'));

    layer.clear();

    // All cells should be None
    for r in 0..3 {
        for c in 0..3 {
            assert!(layer.get_cell(r, c).is_none());
        }
    }
}

#[test]
fn test_layer_write_bytes() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 3, 10);
    layer.write_str(1, 2, "Hello");

    assert_eq!(layer.get_cell(1, 2).unwrap().content, Character::from('H'));
    assert_eq!(layer.get_cell(1, 3).unwrap().content, Character::from('e'));
    assert_eq!(layer.get_cell(1, 4).unwrap().content, Character::from('l'));
    assert_eq!(layer.get_cell(1, 5).unwrap().content, Character::from('l'));
    assert_eq!(layer.get_cell(1, 6).unwrap().content, Character::from('o'));
}

#[test]
fn test_layer_fill_row() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 3, 5);
    layer.fill_row(1, '-', Some(Color::Green), None);

    for c in 0..5 {
        let cell = layer.get_cell(1, c).unwrap();
        assert_eq!(cell.content, Character::from('-'));
        assert_eq!(cell.fg, Some(Color::Green));
    }
}

#[test]
fn test_layer_resize() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 3, 3);
    layer.set_cell(0, 0, Cell::from_char('A'));
    layer.set_cell(2, 2, Cell::from_char('B'));

    // Resize larger
    layer.resize(5, 5);
    assert_eq!(layer.rows(), 5);
    assert_eq!(layer.cols(), 5);
    assert_eq!(layer.get_cell(0, 0).unwrap().content, Character::from('A'));
    assert_eq!(layer.get_cell(2, 2).unwrap().content, Character::from('B'));

    // Resize smaller
    layer.resize(2, 2);
    assert_eq!(layer.rows(), 2);
    assert_eq!(layer.cols(), 2);
    assert_eq!(layer.get_cell(0, 0).unwrap().content, Character::from('A'));
    assert!(layer.get_cell(2, 2).is_none()); // Out of bounds now
}

#[test]
fn test_compositor_creation() {
    let compositor = LayerCompositor::new(24, 80);
    assert_eq!(compositor.rows(), 24);
    assert_eq!(compositor.cols(), 80);
}

#[test]
fn test_compositor_get_layer() {
    let mut compositor = LayerCompositor::new(10, 10);

    // Get a layer (should create it)
    let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    layer.set_cell(0, 0, Cell::from_char('X'));

    // Get the same layer again
    assert!(compositor.get_layer(LayerPriority::CONTENT).is_some());

    // Non-existent layer
    assert!(compositor.get_layer(LayerPriority::HOVER).is_none());
}

#[test]
fn test_compositor_compositing_single_layer() {
    let mut compositor = LayerCompositor::new(3, 3);

    let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    layer.set_cell(0, 0, Cell::from_char('A'));
    layer.set_cell(1, 1, Cell::from_char('B'));

    let composited = compositor.get_composited();
    assert_eq!(composited[0][0].content, Character::from('A'));
    assert_eq!(composited[1][1].content, Character::from('B'));
    assert_eq!(composited[2][2].content, Character::from(' ')); // Empty cells are spaces
}

#[test]
fn test_compositor_layering_order() {
    let mut compositor = LayerCompositor::new(3, 3);

    // Set content layer (lower priority)
    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(1, 1, Cell::from_char('A'));
    }

    // Set floating window layer (higher priority) at same position
    {
        let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);
        layer.set_cell(1, 1, Cell::from_char('B'));
    }

    let composited = compositor.get_composited();

    // Higher priority layer should win
    assert_eq!(composited[1][1].content, Character::from('B'));
}

#[test]
fn test_compositor_transparency() {
    let mut compositor = LayerCompositor::new(3, 3);

    // Set content layer
    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(0, 0, Cell::from_char('A'));
        layer.set_cell(0, 1, Cell::from_char('B'));
        layer.set_cell(0, 2, Cell::from_char('C'));
    }

    // Set floating window layer, but only at position (0, 1)
    // Other positions are transparent (None)
    {
        let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);
        layer.set_cell(0, 1, Cell::from_char('X'));
        // (0, 0) and (0, 2) are None - transparent
    }

    let composited = compositor.get_composited();

    // Position (0, 0): content layer shows through
    assert_eq!(composited[0][0].content, Character::from('A'));
    // Position (0, 1): floating window overrides
    assert_eq!(composited[0][1].content, Character::from('X'));
    // Position (0, 2): content layer shows through
    assert_eq!(composited[0][2].content, Character::from('C'));
}

#[test]
fn test_compositor_colors_preserved() {
    let mut compositor = LayerCompositor::new(3, 3);

    let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    layer.set_cell(
        0,
        0,
        Cell::from_char('A')
            .with_fg(Color::Red)
            .with_bg(Color::Blue),
    );

    let composited = compositor.get_composited();
    assert_eq!(composited[0][0].fg, Some(Color::Red));
    assert_eq!(composited[0][0].bg, Some(Color::Blue));
}

#[test]
fn test_compositor_resize() {
    let mut compositor = LayerCompositor::new(3, 3);

    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(0, 0, Cell::from_char('A'));
    }

    compositor.resize(5, 5);

    assert_eq!(compositor.rows(), 5);
    assert_eq!(compositor.cols(), 5);

    // Content should be preserved
    let composited = compositor.get_composited();
    assert_eq!(composited[0][0].content, Character::from('A'));
}

#[test]
fn test_compositor_clear_layer() {
    let mut compositor = LayerCompositor::new(3, 3);

    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(0, 0, Cell::from_char('A'));
    }

    compositor.clear_layer(LayerPriority::CONTENT);

    // Layer should now be transparent
    let layer = compositor.get_layer(LayerPriority::CONTENT).unwrap();
    assert!(layer.get_cell(0, 0).is_none());
}

#[test]
fn test_compositor_remove_layer() {
    let mut compositor = LayerCompositor::new(3, 3);

    compositor.get_layer_mut(LayerPriority::HOVER);
    assert!(compositor.get_layer(LayerPriority::HOVER).is_some());

    compositor.remove_layer(LayerPriority::HOVER);
    assert!(compositor.get_layer(LayerPriority::HOVER).is_none());
}

#[test]
fn test_layer_dirty_tracking() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 3, 3);
    assert!(layer.is_dirty()); // New layers are dirty

    layer.mark_clean();
    assert!(!layer.is_dirty());

    layer.set_cell(0, 0, Cell::from_char('X'));
    assert!(layer.is_dirty()); // Modified = dirty
}

#[test]
fn test_compositor_multiple_layers() {
    let mut compositor = LayerCompositor::new(5, 10);

    // Content layer: fill with dots
    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        for r in 0..5 {
            for c in 0..10 {
                layer.set_cell(r, c, Cell::from_char('.'));
            }
        }
    }

    // Status bar layer: bottom row
    {
        let layer = compositor.get_layer_mut(LayerPriority::STATUS_BAR);
        layer.fill_row(4, '-', Some(Color::Green), None);
    }

    // Floating window: center box
    {
        let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);
        layer.set_cell(2, 4, Cell::from_char('['));
        layer.set_cell(2, 5, Cell::from_char('O'));
        layer.set_cell(2, 6, Cell::from_char('K'));
        layer.set_cell(2, 7, Cell::from_char(']'));
    }

    let composited = compositor.get_composited();

    // Content shows through where no higher layer
    assert_eq!(composited[0][0].content, Character::from('.'));

    // Status bar overrides content
    assert_eq!(composited[4][0].content, Character::from('-'));
    assert_eq!(composited[4][0].fg, Some(Color::Green));

    // Floating window overrides content
    assert_eq!(composited[2][4].content, Character::from('['));
    assert_eq!(composited[2][5].content, Character::from('O'));
    assert_eq!(composited[2][6].content, Character::from('K'));
    assert_eq!(composited[2][7].content, Character::from(']'));

    // Content still visible around floating window
    assert_eq!(composited[2][3].content, Character::from('.'));
    assert_eq!(composited[2][8].content, Character::from('.'));
}

#[test]
fn test_rect_adjacency() {
    let r1 = Rect::new(0, 0, 1, 1);
    let r2 = Rect::new(0, 2, 1, 3);
    assert!(
        r1.is_adjacent(&r2),
        "Rects touching horizontally should be adjacent"
    );

    let r3 = Rect::new(2, 0, 3, 1);
    assert!(
        r1.is_adjacent(&r3),
        "Rects touching vertically should be adjacent"
    );

    let r4 = Rect::new(3, 3, 4, 4);
    assert!(!r1.is_adjacent(&r4), "Distant rects should not be adjacent");
}

#[test]
fn test_layer_set_cell_dirty_optimization() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 5, 5);
    layer.mark_clean();
    assert!(!layer.is_dirty());

    // Set same value -> should not be dirty
    layer.set_cell(0, 0, Cell::empty()); // Default is None

    // Reset to known state
    layer.set_cell(0, 0, Cell::from_char('A'));
    layer.mark_clean();

    // Set same value
    layer.set_cell(0, 0, Cell::from_char('A'));
    assert!(
        !layer.is_dirty(),
        "Setting same value should not mark dirty"
    );

    // Set different value
    layer.set_cell(0, 0, Cell::from_char('B'));
    assert!(
        layer.is_dirty(),
        "Setting different value should mark dirty"
    );
}

#[test]
fn test_layer_dirty_rects_capping() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 20, 20);
    layer.mark_clean();

    // Add 11 non-overlapping, non-adjacent rects
    for i in 0..11 {
        layer.add_dirty_rect(Rect::new(i * 2, i * 2, i * 2, i * 2));
    }

    // Should have collapsed to 1
    assert_eq!(
        layer.get_dirty_rects().len(),
        1,
        "Should collapse dirty rects when exceeding cap"
    );

    // Check bounding box covers everything
    let rect = layer.get_dirty_rects()[0];
    assert!(rect.start_row <= 0);
    assert!(rect.end_row >= 20); // 10*2 = 20
}

#[test]
fn test_compositor_dirty_rect_optimization() {
    let mut compositor = LayerCompositor::new(5, 5);

    // Setup initial state
    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.fill_rect(Rect::new(0, 0, 4, 4), Cell::from_char('.'));
    }
    compositor.composite(); // Clean everything
    assert!(!compositor.has_dirty());

    // Modify one cell
    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(2, 2, Cell::from_char('X'));
    }

    assert!(compositor.has_dirty());

    // Get composited - logic should only update that one cell + others should remain
    let composited = compositor.get_composited();
    assert_eq!(composited[2][2].content, Character::from('X'));
    assert_eq!(composited[0][0].content, Character::from('.')); // Should still be dot
}
