pub(crate) mod overlay;
pub(crate) mod sixel;

use image::{DynamicImage, ImageDecoder, ImageFormat, RgbaImage};
use std::io::Cursor;

pub(crate) const SNIFF_BYTES: usize = 16;

const MAX_DECODED_SIDE: u32 = 2048;

const SUPPORTED: [ImageFormat; 7] = [
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Gif,
    ImageFormat::Bmp,
    ImageFormat::Ico,
    ImageFormat::WebP,
    ImageFormat::Tiff,
];

/// A decoded image ready for display.
pub(crate) struct DecodedImage {
    format: ImageFormat,
    /// Dimensions of the source image before any downscaling.
    width: u32,
    height: u32,
    file_len: usize,
    pixels: RgbaImage,
}

impl std::fmt::Debug for DecodedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedImage")
            .field("format", &self.format)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("file_len", &self.file_len)
            .finish_non_exhaustive()
    }
}

impl DecodedImage {
    #[must_use]
    pub(crate) const fn pixels(&self) -> &RgbaImage {
        &self.pixels
    }

    /// One-line description shown above the image, e.g. `PNG 640x480, 12.3 KiB`.
    #[must_use]
    pub(crate) fn summary(&self) -> String {
        let name = self
            .format
            .extensions_str()
            .first()
            .map_or_else(|| "image".to_string(), |ext| ext.to_ascii_uppercase());
        format!(
            "{name} {}x{}, {}",
            self.width,
            self.height,
            human_size(self.file_len)
        )
    }
}

#[allow(clippy::cast_precision_loss)] // display-only rounding
fn human_size(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// The image format `prefix` starts with, if it is one rift displays.
#[must_use]
pub(crate) fn sniff(prefix: &[u8]) -> Option<ImageFormat> {
    image::guess_format(prefix)
        .ok()
        .filter(|format| SUPPORTED.contains(format))
}

/// Decodes the first frame with EXIF orientation applied; `None` falls back to text.
#[must_use]
pub(crate) fn decode(bytes: &[u8]) -> Option<DecodedImage> {
    let format = sniff(bytes)?;
    let mut decoder = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_decoder()
        .ok()?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).ok()?;
    if let Some(orientation) = orientation {
        img.apply_orientation(orientation);
    }
    let (width, height) = (img.width(), img.height());
    if width == 0 || height == 0 {
        return None;
    }
    let pixels = if width.max(height) > MAX_DECODED_SIDE {
        img.resize(
            MAX_DECODED_SIDE,
            MAX_DECODED_SIDE,
            image::imageops::FilterType::Triangle,
        )
        .into_rgba8()
    } else {
        img.into_rgba8()
    };
    Some(DecodedImage {
        format,
        width,
        height,
        file_len: bytes.len(),
        pixels,
    })
}

/// Largest aspect-preserving size within `max_w` x `max_h`, never upscaling.
#[must_use]
pub(crate) fn fit(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if width == 0 || height == 0 || max_w == 0 || max_h == 0 {
        return (0, 0);
    }
    if width <= max_w && height <= max_h {
        return (width, height);
    }
    let (w, h, mw, mh) = (
        u64::from(width),
        u64::from(height),
        u64::from(max_w),
        u64::from(max_h),
    );
    // Integer aspect comparison: the side with the larger ratio to its limit binds.
    let (fw, fh) = if w * mh >= h * mw {
        (mw, (h * mw / w).max(1))
    } else {
        ((w * mh / h).max(1), mh)
    };
    (
        u32::try_from(fw).unwrap_or(max_w),
        u32::try_from(fh).unwrap_or(max_h),
    )
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
