use super::DecodedImage;
use crate::error::RiftError;
use crate::layer::{LayerCompositor, Rect};
use crate::term::TerminalBackend;
use std::sync::{Arc, Weak};

const FALLBACK_CELL_PX: (u16, u16) = (10, 20);

const CACHE_ENTRIES: usize = 4;

/// An image to show inside a block of screen cells.
pub(crate) struct ImagePlacement {
    pub(crate) image: Arc<DecodedImage>,
    /// Cells available to the image, in screen coordinates.
    pub(crate) area: Rect,
}

/// Where an image lands this frame.
struct Target {
    image: Arc<DecodedImage>,
    /// Cells the scaled image covers.
    cells: Rect,
    px: (u32, u32),
    /// Per cell of `cells` (row-major): drawn by a layer above content.
    covered: Vec<bool>,
    /// An overlay left some of its cells, which show text, not pixels.
    uncovered: bool,
}

struct Drawn {
    image: Weak<DecodedImage>,
    cells: Rect,
    px: (u32, u32),
    covered: Vec<bool>,
}

impl Drawn {
    fn matches(&self, target: &Target) -> bool {
        self.cells == target.cells
            && self.px == target.px
            && std::ptr::eq(self.image.as_ptr(), Arc::as_ptr(&target.image))
    }
}

struct Encoded {
    image: Weak<DecodedImage>,
    px: (u32, u32),
    sixel: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct ImageOverlay {
    staged: Vec<ImagePlacement>,
    targets: Vec<Target>,
    drawn: Vec<Drawn>,
    /// Most recently used last.
    cache: Vec<Encoded>,
}

impl ImageOverlay {
    /// Images for the next frame. A frame rendered without staging shows none.
    pub(crate) fn stage(&mut self, placements: Vec<ImagePlacement>) {
        self.staged = placements;
    }

    /// Lays out this frame's images and invalidates cells whose pixels are stale.
    pub(crate) fn before_flush<T: TerminalBackend>(
        &mut self,
        term: &T,
        compositor: &mut LayerCompositor,
    ) {
        self.targets.clear();
        let staged = std::mem::take(&mut self.staged);
        if !staged.is_empty() {
            let (cw, ch) = term
                .cell_pixel_size()
                .filter(|&(w, h)| w > 0 && h > 0)
                .unwrap_or(FALLBACK_CELL_PX);
            self.targets
                .extend(staged.into_iter().filter_map(|p| layout(p, cw, ch)));
        }
        for drawn in &self.drawn {
            if !self.targets.iter().any(|t| drawn.matches(t)) {
                compositor.invalidate_rect(drawn.cells);
            }
        }
        // The diff skips unchanged cells, so cells an overlay starts or stops covering are forced.
        for target in &mut self.targets {
            target.covered = compositor.overlay_mask(target.cells);
            let Some(drawn) = self.drawn.iter().find(|d| d.matches(target)) else {
                continue;
            };
            let (rect, now, before) = (target.cells, &target.covered, &drawn.covered);
            let width = rect.end_col - rect.start_col + 1;
            let changed = |row: usize, col: usize| {
                let i = (row - rect.start_row) * width + (col - rect.start_col);
                now[i] != before[i]
            };
            compositor.invalidate_where(rect, changed);
            target.uncovered = before.iter().zip(now).any(|(&was, &is)| was && !is);
        }
    }

    /// Emits images whose cells changed, then repaints overlay cells on top.
    pub(crate) fn after_flush<T: TerminalBackend>(
        &mut self,
        term: &mut T,
        compositor: &mut LayerCompositor,
    ) -> Result<(), RiftError> {
        let mut covered_overlay = false;
        for target in &self.targets {
            let unchanged = self.drawn.iter().any(|d| d.matches(target))
                && !target.uncovered
                && !compositor.wrote_content_within(target.cells);
            if unchanged {
                continue;
            }
            let sixel = encoded(&mut self.cache, target);
            if sixel.is_empty() {
                continue;
            }
            term.hide_cursor()?;
            term.move_cursor(
                u16::try_from(target.cells.start_row).unwrap_or(u16::MAX),
                u16::try_from(target.cells.start_col).unwrap_or(u16::MAX),
            )?;
            term.write(sixel)?;
            covered_overlay |= compositor.invalidate_overlays_within(target.cells);
        }
        if covered_overlay {
            compositor.render_to_terminal(term, false, None)?;
        }

        self.drawn.clear();
        self.drawn.extend(self.targets.drain(..).map(|t| Drawn {
            image: Arc::downgrade(&t.image),
            cells: t.cells,
            px: t.px,
            covered: t.covered,
        }));
        Ok(())
    }
}

/// Scale `placement` to fit its area and center it there, cell-aligned.
fn layout(placement: ImagePlacement, cw: u16, ch: u16) -> Option<Target> {
    let area = placement.area;
    let rows = area.end_row.checked_sub(area.start_row)? + 1;
    let cols = area.end_col.checked_sub(area.start_col)? + 1;
    let max_w = u32::try_from(cols * usize::from(cw)).ok()?;
    // Whole sixel bands only, so no band spills into the row below the area.
    let max_h = u32::try_from(rows * usize::from(ch)).ok()? / 6 * 6;
    let pixels = placement.image.pixels();
    let (w, h) = super::fit(pixels.width(), pixels.height(), max_w, max_h);
    if w == 0 || h == 0 {
        return None;
    }
    let cells_w = (w as usize).div_ceil(usize::from(cw)).min(cols);
    let cells_h = (h as usize).div_ceil(usize::from(ch)).min(rows);
    let row = area.start_row + (rows - cells_h) / 2;
    let col = area.start_col + (cols - cells_w) / 2;
    Some(Target {
        image: placement.image,
        cells: Rect::new(row, col, row + cells_h - 1, col + cells_w - 1),
        px: (w, h),
        covered: Vec::new(),
        uncovered: false,
    })
}

/// The sixel stream for `target`, encoding (and caching) it on a miss.
fn encoded<'a>(cache: &'a mut Vec<Encoded>, target: &Target) -> &'a [u8] {
    cache.retain(|e| e.image.strong_count() > 0);
    let hit = cache.iter().position(|e| {
        e.px == target.px && std::ptr::eq(e.image.as_ptr(), Arc::as_ptr(&target.image))
    });
    let entry = if let Some(i) = hit {
        cache.remove(i)
    } else {
        let pixels = target.image.pixels();
        let sixel = if (pixels.width(), pixels.height()) == target.px {
            super::sixel::encode(pixels)
        } else {
            let scaled = image::imageops::resize(
                pixels,
                target.px.0,
                target.px.1,
                image::imageops::FilterType::Triangle,
            );
            super::sixel::encode(&scaled)
        };
        Encoded {
            image: Arc::downgrade(&target.image),
            px: target.px,
            sixel,
        }
    };
    if cache.len() == CACHE_ENTRIES {
        cache.remove(0);
    }
    cache.push(entry);
    cache.last().map_or(&[], |e| e.sixel.as_slice())
}
