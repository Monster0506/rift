//! Tests for the layer module

use super::*;

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
    let cell = Cell::new(b'A');
    assert_eq!(cell.content, vec![b'A']);
    assert_eq!(cell.fg, None);
    assert_eq!(cell.bg, None);
}

#[test]
fn test_cell_with_colors() {
    let cell = Cell::new(b'X').with_fg(Color::Red).with_bg(Color::Blue);
    assert_eq!(cell.content, vec![b'X']);
    assert_eq!(cell.fg, Some(Color::Red));
    assert_eq!(cell.bg, Some(Color::Blue));
}

#[test]
fn test_cell_empty() {
    let cell = Cell::empty();
    assert_eq!(cell.content, vec![b' ']);
}

#[test]
fn test_cell_from_bytes() {
    let cell = Cell::from_bytes("│".as_bytes());
    assert_eq!(cell.content, "│".as_bytes());
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
    assert!(layer.set_cell(2, 3, Cell::new(b'X')));

    // Get the cell back
    let cell = layer.get_cell(2, 3);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'X']);

    // Out of bounds should return None
    assert!(layer.get_cell(10, 10).is_none());
}

#[test]
fn test_layer_out_of_bounds() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 5, 5);

    // Out of bounds set should return false
    assert!(!layer.set_cell(10, 10, Cell::new(b'X')));
    assert!(!layer.set_cell(5, 3, Cell::new(b'X')));
    assert!(!layer.set_cell(3, 5, Cell::new(b'X')));
}

#[test]
fn test_layer_clear() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 3, 3);
    layer.set_cell(1, 1, Cell::new(b'X'));

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
    layer.write_bytes(1, 2, b"Hello");

    assert_eq!(layer.get_cell(1, 2).unwrap().content, vec![b'H']);
    assert_eq!(layer.get_cell(1, 3).unwrap().content, vec![b'e']);
    assert_eq!(layer.get_cell(1, 4).unwrap().content, vec![b'l']);
    assert_eq!(layer.get_cell(1, 5).unwrap().content, vec![b'l']);
    assert_eq!(layer.get_cell(1, 6).unwrap().content, vec![b'o']);
}

#[test]
fn test_layer_fill_row() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 3, 5);
    layer.fill_row(1, b'-', Some(Color::Green), None);

    for c in 0..5 {
        let cell = layer.get_cell(1, c).unwrap();
        assert_eq!(cell.content, vec![b'-']);
        assert_eq!(cell.fg, Some(Color::Green));
    }
}

#[test]
fn test_layer_resize() {
    let mut layer = Layer::new(LayerPriority::CONTENT, 3, 3);
    layer.set_cell(0, 0, Cell::new(b'A'));
    layer.set_cell(2, 2, Cell::new(b'B'));

    // Resize larger
    layer.resize(5, 5);
    assert_eq!(layer.rows(), 5);
    assert_eq!(layer.cols(), 5);
    assert_eq!(layer.get_cell(0, 0).unwrap().content, vec![b'A']);
    assert_eq!(layer.get_cell(2, 2).unwrap().content, vec![b'B']);

    // Resize smaller
    layer.resize(2, 2);
    assert_eq!(layer.rows(), 2);
    assert_eq!(layer.cols(), 2);
    assert_eq!(layer.get_cell(0, 0).unwrap().content, vec![b'A']);
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
    layer.set_cell(0, 0, Cell::new(b'X'));

    // Get the same layer again
    assert!(compositor.get_layer(LayerPriority::CONTENT).is_some());

    // Non-existent layer
    assert!(compositor.get_layer(LayerPriority::HOVER).is_none());
}

#[test]
fn test_compositor_compositing_single_layer() {
    let mut compositor = LayerCompositor::new(3, 3);

    let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    layer.set_cell(0, 0, Cell::new(b'A'));
    layer.set_cell(1, 1, Cell::new(b'B'));

    let composited = compositor.get_composited();
    assert_eq!(composited[0][0].content, vec![b'A']);
    assert_eq!(composited[1][1].content, vec![b'B']);
    assert_eq!(composited[2][2].content, vec![b' ']); // Empty cells are spaces
}

#[test]
fn test_compositor_layering_order() {
    let mut compositor = LayerCompositor::new(3, 3);

    // Set content layer (lower priority)
    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(1, 1, Cell::new(b'A'));
    }

    // Set floating window layer (higher priority) at same position
    {
        let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);
        layer.set_cell(1, 1, Cell::new(b'B'));
    }

    let composited = compositor.get_composited();

    // Higher priority layer should win
    assert_eq!(composited[1][1].content, vec![b'B']);
}

#[test]
fn test_compositor_transparency() {
    let mut compositor = LayerCompositor::new(3, 3);

    // Set content layer
    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(0, 0, Cell::new(b'A'));
        layer.set_cell(0, 1, Cell::new(b'B'));
        layer.set_cell(0, 2, Cell::new(b'C'));
    }

    // Set floating window layer, but only at position (0, 1)
    // Other positions are transparent (None)
    {
        let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);
        layer.set_cell(0, 1, Cell::new(b'X'));
        // (0, 0) and (0, 2) are None - transparent
    }

    let composited = compositor.get_composited();

    // Position (0, 0): content layer shows through
    assert_eq!(composited[0][0].content, vec![b'A']);
    // Position (0, 1): floating window overrides
    assert_eq!(composited[0][1].content, vec![b'X']);
    // Position (0, 2): content layer shows through
    assert_eq!(composited[0][2].content, vec![b'C']);
}

#[test]
fn test_compositor_colors_preserved() {
    let mut compositor = LayerCompositor::new(3, 3);

    let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
    layer.set_cell(
        0,
        0,
        Cell::new(b'A').with_fg(Color::Red).with_bg(Color::Blue),
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
        layer.set_cell(0, 0, Cell::new(b'A'));
    }

    compositor.resize(5, 5);

    assert_eq!(compositor.rows(), 5);
    assert_eq!(compositor.cols(), 5);

    // Content should be preserved
    let composited = compositor.get_composited();
    assert_eq!(composited[0][0].content, vec![b'A']);
}

#[test]
fn test_compositor_clear_layer() {
    let mut compositor = LayerCompositor::new(3, 3);

    {
        let layer = compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(0, 0, Cell::new(b'A'));
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

    layer.set_cell(0, 0, Cell::new(b'X'));
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
                layer.set_cell(r, c, Cell::new(b'.'));
            }
        }
    }

    // Status bar layer: bottom row
    {
        let layer = compositor.get_layer_mut(LayerPriority::STATUS_BAR);
        layer.fill_row(4, b'-', Some(Color::Green), None);
    }

    // Floating window: center box
    {
        let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);
        layer.set_cell(2, 4, Cell::new(b'['));
        layer.set_cell(2, 5, Cell::new(b'O'));
        layer.set_cell(2, 6, Cell::new(b'K'));
        layer.set_cell(2, 7, Cell::new(b']'));
    }

    let composited = compositor.get_composited();

    // Content shows through where no higher layer
    assert_eq!(composited[0][0].content, vec![b'.']);

    // Status bar overrides content
    assert_eq!(composited[4][0].content, vec![b'-']);
    assert_eq!(composited[4][0].fg, Some(Color::Green));

    // Floating window overrides content
    assert_eq!(composited[2][4].content, vec![b'[']);
    assert_eq!(composited[2][5].content, vec![b'O']);
    assert_eq!(composited[2][6].content, vec![b'K']);
    assert_eq!(composited[2][7].content, vec![b']']);

    // Content still visible around floating window
    assert_eq!(composited[2][3].content, vec![b'.']);
    assert_eq!(composited[2][8].content, vec![b'.']);
}
