use super::*;
use image::{ImageEncoder, Rgba};

pub(crate) fn png_bytes(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    out
}

/// Minimal sixel decoder: pixel grid of RGB percentages, `None` = untouched.
fn decode_sixel(data: &[u8]) -> (usize, usize, Vec<Vec<Option<[usize; 3]>>>) {
    let body = data
        .strip_prefix(b"\x1bP0;1;0q")
        .and_then(|d| d.strip_suffix(b"\x1b\\"))
        .expect("DCS framing");
    let mut pos = 0;
    let num = |pos: &mut usize| {
        let start = *pos;
        while body[*pos].is_ascii_digit() {
            *pos += 1;
        }
        std::str::from_utf8(&body[start..*pos])
            .unwrap()
            .parse::<usize>()
            .unwrap()
    };
    assert_eq!(body[pos], b'"');
    pos += 1;
    let mut raster = Vec::new();
    for _ in 0..4 {
        raster.push(num(&mut pos));
        if body[pos] == b';' {
            pos += 1;
        }
    }
    let (width, height) = (raster[2], raster[3]);
    let mut grid = vec![vec![None; width]; height];
    let mut palette = std::collections::HashMap::new();
    let (mut col, mut band, mut color) = (0usize, 0usize, 0usize);
    while pos < body.len() {
        match body[pos] {
            b'#' => {
                pos += 1;
                color = num(&mut pos);
                if body.get(pos) == Some(&b';') {
                    pos += 3; // ";2;"
                    let red = num(&mut pos);
                    pos += 1;
                    let green = num(&mut pos);
                    pos += 1;
                    let blue = num(&mut pos);
                    palette.insert(color, [red, green, blue]);
                }
            }
            b'$' => {
                col = 0;
                pos += 1;
            }
            b'-' => {
                col = 0;
                band += 1;
                pos += 1;
            }
            b'!' => {
                pos += 1;
                let count = num(&mut pos);
                let bits = body[pos] - 0x3F;
                for _ in 0..count {
                    paint(&mut grid, col, band, bits, palette[&color]);
                    col += 1;
                }
                pos += 1;
            }
            ch @ 0x3F..=0x7E => {
                paint(&mut grid, col, band, ch - 0x3F, palette[&color]);
                col += 1;
                pos += 1;
            }
            other => panic!("unexpected byte {other:#x}"),
        }
    }
    (width, height, grid)
}

fn paint(grid: &mut [Vec<Option<[usize; 3]>>], x: usize, band: usize, bits: u8, rgb: [usize; 3]) {
    for dy in 0..6 {
        if bits & (1 << dy) != 0 {
            grid[band * 6 + dy][x] = Some(rgb);
        }
    }
}

fn pct(rgb: [u8; 3]) -> [usize; 3] {
    rgb.map(|c| (usize::from(c) * 100 + 127) / 255)
}

#[test]
fn sixel_round_trips_exact_colors_and_transparency() {
    // 13 rows spans three bands, including a partial last band.
    let red = [200, 10, 10];
    let blue = [0, 0, 255];
    let img = RgbaImage::from_fn(9, 13, |x, y| {
        if x == 4 && y == 6 {
            Rgba([0, 0, 0, 0])
        } else if (x + y) % 3 == 0 {
            Rgba([red[0], red[1], red[2], 255])
        } else {
            Rgba([blue[0], blue[1], blue[2], 255])
        }
    });

    let sixel = sixel::encode(&img);
    let (w, h, grid) = decode_sixel(&sixel);

    assert_eq!((w, h), (9, 13));
    for (y, row) in grid.iter().enumerate() {
        for (x, &pixel) in row.iter().enumerate() {
            let expected = if x == 4 && y == 6 {
                None
            } else if (x + y) % 3 == 0 {
                Some(pct(red))
            } else {
                Some(pct(blue))
            };
            assert_eq!(pixel, expected, "pixel ({x}, {y})");
        }
    }
    assert!(
        !sixel.ends_with(b"-\x1b\\"),
        "a graphics newline after the last band can scroll the screen"
    );
}

#[test]
fn sixel_quantizes_photos_to_256_close_colors() {
    let rgb = |x: usize, y: usize| [x * 4, y * 4, (x + y) * 2].map(|c| u8::try_from(c).unwrap());
    let img = RgbaImage::from_fn(64, 64, |x, y| {
        let [r, g, b] = rgb(usize::try_from(x).unwrap(), usize::try_from(y).unwrap());
        Rgba([r, g, b, 255])
    });

    let (w, h, grid) = decode_sixel(&sixel::encode(&img));

    assert_eq!((w, h), (64, 64));
    let colors: std::collections::HashSet<_> = grid.iter().flatten().flatten().collect();
    assert!(colors.len() <= 256);
    // Mean per-channel error must stay under 2% (sixel color units).
    let samples = 64 * 64 * 3;
    let mut error = 0;
    for (y, row) in grid.iter().enumerate() {
        for (x, pixel) in row.iter().enumerate() {
            let got = pixel.expect("opaque pixel drawn");
            let want = pct(rgb(x, y));
            error += (0..3).map(|c| got[c].abs_diff(want[c])).sum::<usize>();
        }
    }
    assert!(
        error < 2 * samples,
        "total channel error {error} over {samples} samples"
    );
}

#[test]
fn decode_reads_png_and_describes_it() {
    let img = RgbaImage::from_pixel(30, 20, Rgba([1, 2, 3, 255]));
    let bytes = png_bytes(&img);

    let decoded = decode(&bytes).expect("png decodes");

    assert_eq!(decoded.pixels().dimensions(), (30, 20));
    assert!(
        decoded.summary().starts_with("PNG 30x20, "),
        "{}",
        decoded.summary()
    );
}

#[test]
fn decode_rejects_text_and_corrupt_images() {
    assert!(sniff(b"P1 priority: fix the parser\n").is_none());
    assert!(decode(b"fn main() {}\n").is_none());
    let mut corrupt = b"\x89PNG\r\n\x1a\n".to_vec();
    corrupt.extend_from_slice(b"definitely not chunks");
    assert!(sniff(&corrupt).is_some());
    assert!(decode(&corrupt).is_none());
}

#[test]
fn fit_preserves_aspect_and_never_upscales() {
    assert_eq!(fit(100, 50, 400, 400), (100, 50));
    assert_eq!(fit(1000, 500, 400, 400), (400, 200));
    assert_eq!(fit(500, 1000, 400, 400), (200, 400));
    assert_eq!(fit(1000, 1, 10, 10), (10, 1));
    assert_eq!(fit(10, 10, 0, 10), (0, 0));
}

/// One frame as `RenderSystem::render` drives it.
fn frame(
    overlay: &mut overlay::ImageOverlay,
    compositor: &mut crate::layer::LayerCompositor,
    term: &mut crate::test_utils::MockTerminal,
    placements: Vec<overlay::ImagePlacement>,
) {
    overlay.stage(placements);
    overlay.before_flush(term, compositor);
    compositor.render_to_terminal(term, false, &[]).unwrap();
    overlay.after_flush(term, compositor).unwrap();
}

#[test]
fn overlay_emits_once_repaints_popups_and_clears_when_removed() {
    use crate::layer::{Cell, LayerCompositor, LayerPriority, Rect};

    let image = std::sync::Arc::new(
        decode(&png_bytes(&RgbaImage::from_pixel(
            100,
            100,
            Rgba([9, 9, 9, 255]),
        )))
        .unwrap(),
    );
    let placement = || overlay::ImagePlacement {
        image: std::sync::Arc::clone(&image),
        area: Rect::new(0, 0, 4, 9),
    };
    let mut compositor = LayerCompositor::new(10, 20);
    let mut term = crate::test_utils::MockTerminal::new(10, 20);
    let mut overlay = overlay::ImageOverlay::default();

    frame(&mut overlay, &mut compositor, &mut term, vec![placement()]);
    assert!(term.get_written_string().contains("\x1bP0;1;0q"));

    term.clear();
    frame(&mut overlay, &mut compositor, &mut term, vec![placement()]);
    assert!(
        term.get_written_string().is_empty(),
        "an unchanged image must not be re-sent"
    );

    // A popup over the image replaces only its own cells: no re-send.
    term.clear();
    compositor
        .get_layer_mut(LayerPriority::NOTIFICATION)
        .set_cell(2, 3, Cell::from_char('N'));
    frame(&mut overlay, &mut compositor, &mut term, vec![placement()]);
    let written = term.get_written_string();
    assert!(written.contains('N'));
    assert!(!written.contains("\x1bP"), "popup alone must not re-send");

    // A content cell rewritten under the image forces a re-send with the popup on top.
    term.clear();
    compositor.invalidate_rect(Rect::new(0, 0, 0, 0));
    frame(&mut overlay, &mut compositor, &mut term, vec![placement()]);
    let written = term.get_written_string();
    let dcs = written.find("\x1bP").expect("image re-sent");
    assert!(
        written[dcs..].contains('N'),
        "popup must be painted after the image: {written:?}"
    );

    // Closing the popup blanks its cells, so the image is re-sent to fill them.
    term.clear();
    compositor.clear_layer(LayerPriority::NOTIFICATION);
    frame(&mut overlay, &mut compositor, &mut term, vec![placement()]);
    assert!(term.get_written_string().contains("\x1bP"));

    // Once the image goes away, every cell it covered is rewritten.
    term.clear();
    frame(&mut overlay, &mut compositor, &mut term, Vec::new());
    let rows: std::collections::HashSet<u16> =
        term.cursor_moves.iter().map(|&(row, _)| row).collect();
    assert!(
        (0..5).all(|r| rows.contains(&r)),
        "rewritten rows: {rows:?}"
    );
    assert!(!term.get_written_string().contains("\x1bP"));
}

#[test]
fn overlay_blank_popup_cells_still_cover_the_image() {
    use crate::layer::{Cell, LayerCompositor, LayerPriority, Rect};

    let image = std::sync::Arc::new(
        decode(&png_bytes(&RgbaImage::from_pixel(
            100,
            100,
            Rgba([9, 9, 9, 255]),
        )))
        .unwrap(),
    );
    let placement = || overlay::ImagePlacement {
        image: std::sync::Arc::clone(&image),
        area: Rect::new(0, 0, 4, 9),
    };
    let mut compositor = LayerCompositor::new(10, 20);
    let mut term = crate::test_utils::MockTerminal::new(10, 20);
    let mut overlay = overlay::ImageOverlay::default();
    frame(&mut overlay, &mut compositor, &mut term, vec![placement()]);

    // Blank popup cells equal the content beneath, so only coverage tracking paints them.
    term.clear();
    compositor
        .get_layer_mut(LayerPriority::FLOATING_WINDOW)
        .set_cell(2, 3, Cell::empty());
    frame(&mut overlay, &mut compositor, &mut term, vec![placement()]);
    assert!(
        term.cursor_moves.contains(&(2, 3)),
        "popup cell not painted over the image: {:?}",
        term.cursor_moves
    );
    assert!(!term.get_written_string().contains("\x1bP"));

    // Closing it changes no cell either, yet the image must be re-sent there.
    term.clear();
    compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
    frame(&mut overlay, &mut compositor, &mut term, vec![placement()]);
    assert!(term.get_written_string().contains("\x1bP"));
}
