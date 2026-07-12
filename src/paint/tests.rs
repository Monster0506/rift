use super::*;
use crate::layer::{Layer, LayerPriority};

fn cell(ch: char, fg: Option<Color>) -> Cell {
    Cell::from_char(ch).with_colors(fg, None)
}

#[test]
fn adjacent_same_style_writes_coalesce_into_one_run() {
    let mut frame = PaintFrame::new(1);
    frame.set_cell(0, 0, cell('a', Some(Color::Red)));
    frame.set_cell(0, 1, cell('b', Some(Color::Red)));
    frame.set_cell(0, 2, cell('c', Some(Color::Red)));

    assert_eq!(frame.rows[0].runs.len(), 1);
    assert_eq!(
        frame.rows[0].runs[0].chars,
        vec![
            Character::from('a'),
            Character::from('b'),
            Character::from('c')
        ]
    );
}

#[test]
fn style_change_opens_a_new_run() {
    let mut frame = PaintFrame::new(1);
    frame.set_cell(0, 0, cell('a', Some(Color::Red)));
    frame.set_cell(0, 1, cell('b', Some(Color::Blue)));

    assert_eq!(frame.rows[0].runs.len(), 2);
    assert_eq!(frame.rows[0].runs[1].col, 1);
}

#[test]
fn non_adjacent_column_opens_a_new_run() {
    let mut frame = PaintFrame::new(1);
    frame.set_cell(0, 0, cell('a', None));
    frame.set_cell(0, 5, cell('b', None));

    assert_eq!(frame.rows[0].runs.len(), 2);
    assert_eq!(frame.rows[0].runs[1].col, 5);
}

#[test]
fn out_of_bounds_row_is_silently_ignored() {
    let mut frame = PaintFrame::new(1);
    frame.set_cell(5, 0, cell('a', None));
    assert_eq!(frame.rows.len(), 1);
}

#[test]
fn later_overlapping_write_replays_after_the_earlier_one() {
    // Same shape as an overlay adornment redrawing part of an already-painted
    // span: the second run must rasterize after the first so it wins.
    let mut frame = PaintFrame::new(1);
    for (i, ch) in "hello".chars().enumerate() {
        frame.set_cell(0, i, cell(ch, Some(Color::White)));
    }
    frame.set_cell(0, 1, cell('X', Some(Color::Yellow)));

    let mut layer = Layer::new(LayerPriority::CONTENT, 1, 10);
    rasterize(&frame, &mut layer);

    assert_eq!(layer.get_cell(0, 0).unwrap().to_char(), 'h');
    assert_eq!(layer.get_cell(0, 1).unwrap().to_char(), 'X');
    assert_eq!(layer.get_cell(0, 1).unwrap().fg, Some(Color::Yellow));
    assert_eq!(layer.get_cell(0, 2).unwrap().to_char(), 'l');
}

#[test]
fn rasterize_matches_writing_directly_to_a_layer() {
    let mut direct = Layer::new(LayerPriority::CONTENT, 3, 8);
    let writes: &[(usize, usize, char, Option<Color>)] = &[
        (0, 0, 'h', Some(Color::Red)),
        (0, 1, 'i', Some(Color::Red)),
        (0, 2, ' ', None),
        (1, 0, 'x', Some(Color::Blue)),
        (1, 3, 'y', Some(Color::Green)),
        (2, 7, 'z', None),
    ];
    for &(row, col, ch, fg) in writes {
        direct.set_cell(row, col, cell(ch, fg));
    }

    let mut frame = PaintFrame::new(3);
    for &(row, col, ch, fg) in writes {
        frame.set_cell(row, col, cell(ch, fg));
    }
    let mut via_frame = Layer::new(LayerPriority::CONTENT, 3, 8);
    rasterize(&frame, &mut via_frame);

    for row in 0..3 {
        for col in 0..8 {
            assert_eq!(
                direct.get_cell(row, col),
                via_frame.get_cell(row, col),
                "mismatch at ({row},{col})"
            );
        }
    }
}

#[test]
fn write_str_colored_paints_one_cell_per_char() {
    let mut frame = PaintFrame::new(1);
    frame.write_str_colored(0, 2, "hi", Some(Color::Green), None);

    let mut layer = Layer::new(LayerPriority::CONTENT, 1, 6);
    rasterize(&frame, &mut layer);

    assert_eq!(layer.get_cell(0, 2).unwrap().to_char(), 'h');
    assert_eq!(layer.get_cell(0, 3).unwrap().to_char(), 'i');
    assert_eq!(layer.get_cell(0, 3).unwrap().fg, Some(Color::Green));
}

#[test]
fn preserves_raw_byte_content_that_is_not_valid_utf8() {
    let mut frame = PaintFrame::new(1);
    frame.set_cell(0, 0, Cell::new(Character::Byte(0xFF)));

    let mut layer = Layer::new(LayerPriority::CONTENT, 1, 4);
    rasterize(&frame, &mut layer);

    assert_eq!(layer.get_cell(0, 0).unwrap().content, Character::Byte(0xFF));
}
