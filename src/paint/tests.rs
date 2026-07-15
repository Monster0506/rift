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
fn reset_reuses_run_char_buffers_without_leaking_stale_content() {
    let mut frame = PaintFrame::new(1);
    // Open several runs so multiple `chars` buffers get pooled on reset.
    frame.set_cell(0, 0, cell('a', Some(Color::Red)));
    frame.set_cell(0, 1, cell('b', Some(Color::Blue)));
    frame.set_cell(0, 2, cell('c', Some(Color::Green)));
    assert_eq!(frame.rows[0].runs.len(), 3);

    frame.reset(1);
    assert!(frame.rows[0].runs.is_empty());
    assert!(
        !frame.char_buf_pool.is_empty(),
        "reset should have pooled the emptied run buffers instead of dropping them"
    );

    // A shorter run reusing a pooled buffer must not show leftover chars
    // from the buffer's previous life.
    frame.set_cell(0, 0, cell('z', None));
    assert_eq!(frame.rows[0].runs.len(), 1);
    assert_eq!(frame.rows[0].runs[0].chars, vec![Character::from('z')]);
}

#[test]
fn reset_across_many_cycles_matches_a_fresh_frame() {
    // Cross-frame reuse must be behaviorally invisible: N resets then a
    // paint pass must rasterize identically to a brand-new frame.
    let mut reused = PaintFrame::new(2);
    for _ in 0..5 {
        reused.reset(2);
        for (i, ch) in "hi".chars().enumerate() {
            reused.set_cell(0, i, cell(ch, Some(Color::Red)));
        }
        reused.set_cell(1, 0, cell('!', Some(Color::Blue)));
    }

    let mut fresh = PaintFrame::new(2);
    for (i, ch) in "hi".chars().enumerate() {
        fresh.set_cell(0, i, cell(ch, Some(Color::Red)));
    }
    fresh.set_cell(1, 0, cell('!', Some(Color::Blue)));

    assert_eq!(reused, fresh);
}

#[test]
fn reset_row_clears_only_the_target_row() {
    let mut frame = PaintFrame::new(3);
    frame.set_cell(0, 0, cell('a', Some(Color::Red)));
    frame.set_cell(1, 0, cell('b', Some(Color::Blue)));
    frame.set_cell(2, 0, cell('c', Some(Color::Green)));

    frame.reset_row(1);

    assert_eq!(frame.rows[0].runs.len(), 1, "row 0 must be untouched");
    assert!(frame.rows[1].runs.is_empty(), "row 1 must be cleared");
    assert_eq!(frame.rows[2].runs.len(), 1, "row 2 must be untouched");
    assert!(
        !frame.char_buf_pool.is_empty(),
        "reset_row should pool the emptied row's buffer, same as reset()"
    );
}

#[test]
fn reset_row_out_of_bounds_is_silently_ignored() {
    let mut frame = PaintFrame::new(1);
    frame.set_cell(0, 0, cell('a', None));
    frame.reset_row(5);
    assert_eq!(
        frame.rows[0].runs.len(),
        1,
        "in-bounds row must be untouched"
    );
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
