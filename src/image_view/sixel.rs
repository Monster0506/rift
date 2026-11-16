use image::RgbaImage;
use std::collections::HashMap;
use std::io::Write as _;

const MAX_COLORS: usize = 256;
const TRANSPARENT: u16 = u16::MAX;
const BUCKET_BITS: u32 = 5;
const BUCKETS: usize = 1 << (3 * BUCKET_BITS);

const fn is_opaque(px: [u8; 4]) -> bool {
    px[3] >= 128
}

/// Per-pixel palette indices (`TRANSPARENT` for see-through) and the RGB palette.
fn quantize(img: &RgbaImage) -> (Vec<u16>, Vec<[u8; 3]>) {
    let mut exact: HashMap<[u8; 3], u16> = HashMap::new();
    let mut overflow = false;
    for px in img.pixels() {
        if !is_opaque(px.0) {
            continue;
        }
        let rgb = [px.0[0], px.0[1], px.0[2]];
        if !exact.contains_key(&rgb) {
            if exact.len() == MAX_COLORS {
                overflow = true;
                break;
            }
            let next = u16::try_from(exact.len()).unwrap_or(TRANSPARENT);
            exact.insert(rgb, next);
        }
    }

    if !overflow {
        let mut palette = vec![[0u8; 3]; exact.len()];
        for (rgb, &idx) in &exact {
            palette[usize::from(idx)] = *rgb;
        }
        let indices = img
            .pixels()
            .map(|px| {
                if is_opaque(px.0) {
                    exact[&[px.0[0], px.0[1], px.0[2]]]
                } else {
                    TRANSPARENT
                }
            })
            .collect();
        return (indices, palette);
    }

    median_cut(img)
}

const fn bucket_of(px: [u8; 4]) -> usize {
    let shift = 8 - BUCKET_BITS;
    ((px[0] as usize >> shift) << (2 * BUCKET_BITS))
        | ((px[1] as usize >> shift) << BUCKET_BITS)
        | (px[2] as usize >> shift)
}

/// A box of histogram buckets, bounds inclusive per channel.
#[derive(Clone, Copy)]
struct ColorBox {
    lo: [usize; 3],
    hi: [usize; 3],
    pixels: u64,
}

impl ColorBox {
    fn buckets(self) -> impl Iterator<Item = usize> {
        (self.lo[0]..=self.hi[0]).flat_map(move |r| {
            (self.lo[1]..=self.hi[1]).flat_map(move |g| {
                (self.lo[2]..=self.hi[2])
                    .map(move |b| (r << (2 * BUCKET_BITS)) | (g << BUCKET_BITS) | b)
            })
        })
    }

    /// The widest channel and its extent.
    fn longest_axis(self) -> (usize, usize) {
        (0..3)
            .map(|axis| (axis, self.hi[axis] - self.lo[axis]))
            .max_by_key(|&(_, extent)| extent)
            .unwrap_or((0, 0))
    }
}

/// Channel coordinates of histogram bucket `key`.
const fn coords(key: usize) -> [usize; 3] {
    let mask = (1 << BUCKET_BITS) - 1;
    [
        key >> (2 * BUCKET_BITS),
        (key >> BUCKET_BITS) & mask,
        key & mask,
    ]
}

/// Shrink `bounds` to the occupied buckets inside it; `None` if empty.
fn tighten(hist: &[[u32; 4]], bounds: ColorBox) -> Option<ColorBox> {
    let mut out = ColorBox {
        lo: [usize::MAX; 3],
        hi: [0; 3],
        pixels: 0,
    };
    for key in bounds.buckets() {
        let count = hist[key][0];
        if count == 0 {
            continue;
        }
        let c = coords(key);
        for axis in 0..3 {
            out.lo[axis] = out.lo[axis].min(c[axis]);
            out.hi[axis] = out.hi[axis].max(c[axis]);
        }
        out.pixels += u64::from(count);
    }
    (out.pixels > 0).then_some(out)
}

/// Split at the pixel median along the widest channel.
fn split(hist: &[[u32; 4]], bx: ColorBox) -> Option<(ColorBox, ColorBox)> {
    let (axis, extent) = bx.longest_axis();
    if extent == 0 {
        return None;
    }
    let mut per_slice = [0u64; 1 << BUCKET_BITS];
    for key in bx.buckets() {
        per_slice[coords(key)[axis]] += u64::from(hist[key][0]);
    }
    let mut seen = 0;
    let mut cut = bx.lo[axis];
    for (slice, &count) in per_slice
        .iter()
        .enumerate()
        .take(bx.hi[axis])
        .skip(bx.lo[axis])
    {
        seen += count;
        cut = slice;
        if seen * 2 >= bx.pixels {
            break;
        }
    }
    let (mut left, mut right) = (bx, bx);
    left.hi[axis] = cut;
    right.lo[axis] = cut + 1;
    Some((tighten(hist, left)?, tighten(hist, right)?))
}

fn median_cut(img: &RgbaImage) -> (Vec<u16>, Vec<[u8; 3]>) {
    // Per bucket: pixel count and channel sums, for exact box means.
    let mut hist = vec![[0u32; 4]; BUCKETS];
    for px in img.pixels() {
        if is_opaque(px.0) {
            let h = &mut hist[bucket_of(px.0)];
            h[0] += 1;
            h[1] += u32::from(px.0[0]);
            h[2] += u32::from(px.0[1]);
            h[3] += u32::from(px.0[2]);
        }
    }
    let full = ColorBox {
        lo: [0; 3],
        hi: [(1 << BUCKET_BITS) - 1; 3],
        pixels: 0,
    };
    let mut boxes: Vec<ColorBox> = tighten(&hist, full).into_iter().collect();
    while boxes.len() < MAX_COLORS {
        // Split the box with the most pixels times spread.
        let Some((idx, _)) = boxes
            .iter()
            .enumerate()
            .map(|(i, b)| (i, b.pixels * b.longest_axis().1 as u64))
            .filter(|&(_, score)| score > 0)
            .max_by_key(|&(_, score)| score)
        else {
            break;
        };
        let Some((left, right)) = split(&hist, boxes[idx]) else {
            break;
        };
        boxes[idx] = left;
        boxes.push(right);
    }

    let mut lut = vec![0u16; BUCKETS];
    let mut palette = Vec::with_capacity(boxes.len());
    for (i, bx) in boxes.iter().enumerate() {
        let index = u16::try_from(i).unwrap_or(0);
        let mut sum = [0u64; 3];
        for key in bx.buckets() {
            lut[key] = index;
            for (s, &v) in sum.iter_mut().zip(&hist[key][1..]) {
                *s += u64::from(v);
            }
        }
        palette.push(sum.map(|s| u8::try_from(s / bx.pixels).unwrap_or(u8::MAX)));
    }
    let indices = img
        .pixels()
        .map(|px| {
            if is_opaque(px.0) {
                lut[bucket_of(px.0)]
            } else {
                TRANSPARENT
            }
        })
        .collect();
    (indices, palette)
}

/// Append `count` repeats of sixel `ch`, run-length encoded when shorter.
fn push_run(out: &mut Vec<u8>, ch: u8, count: usize) {
    if count > 3 {
        let _ = write!(out, "!{count}");
        out.push(ch);
    } else {
        out.extend(std::iter::repeat_n(ch, count));
    }
}

/// Encodes `img` as a sixel DCS sequence; empty if nothing is opaque.
#[must_use]
pub(crate) fn encode(img: &RgbaImage) -> Vec<u8> {
    let (width, height) = (img.width() as usize, img.height() as usize);
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let (indices, palette) = quantize(img);
    if palette.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(width * height / 2);
    // P2=1 leaves unset pixels transparent; raster attributes give 1:1 aspect and exact size.
    let _ = write!(out, "\x1bP0;1;0q\"1;1;{width};{height}");
    for (i, [r, g, b]) in palette.iter().enumerate() {
        let pct = |c: u8| (u32::from(c) * 100 + 127) / 255;
        let _ = write!(out, "#{i};2;{};{};{}", pct(*r), pct(*g), pct(*b));
    }

    // Per band, one row of 6-bit masks per color present, built in one pass.
    let mut slot_of = vec![u16::MAX; palette.len()];
    let mut slot_colors: Vec<u16> = Vec::new();
    let mut masks: Vec<u8> = Vec::new();
    for band_top in (0..height).step_by(6) {
        let band_rows = (height - band_top).min(6);
        for &color in &slot_colors {
            slot_of[usize::from(color)] = u16::MAX;
        }
        slot_colors.clear();
        masks.clear();

        for dy in 0..band_rows {
            let row = &indices[(band_top + dy) * width..(band_top + dy + 1) * width];
            for (x, &color) in row.iter().enumerate() {
                if color == TRANSPARENT {
                    continue;
                }
                let mut slot = slot_of[usize::from(color)];
                if slot == u16::MAX {
                    slot = u16::try_from(slot_colors.len()).unwrap_or(0);
                    slot_of[usize::from(color)] = slot;
                    slot_colors.push(color);
                    masks.resize(masks.len() + width, 0);
                }
                masks[usize::from(slot) * width + x] |= 1 << dy;
            }
        }

        for (slot, &color) in slot_colors.iter().enumerate() {
            if slot > 0 {
                out.push(b'$');
            }
            let _ = write!(out, "#{color}");
            let row = &masks[slot * width..(slot + 1) * width];
            // Trailing empty sixels draw nothing; drop them.
            let used = row.iter().rposition(|&m| m != 0).map_or(0, |p| p + 1);
            let mut x = 0;
            while x < used {
                let mask = row[x];
                let run = row[x..used].iter().take_while(|&&m| m == mask).count();
                push_run(&mut out, 0x3F + mask, run);
                x += run;
            }
        }
        // No graphics newline after the last band: it could scroll the screen.
        if band_top + 6 < height {
            out.push(b'-');
        }
    }
    out.extend_from_slice(b"\x1b\\");
    out
}
