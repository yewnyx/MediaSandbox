use std::io::Cursor;
use image::{imageops, DynamicImage, ImageFormat, ImageReader, RgbaImage};

// TODO(perf): decode at target size without a full-resolution intermediate.
// JPEG supports DCT scaling (1/2, 1/4, 1/8) via zune-jpeg; PNG can be
// downsampled scanline-by-scanline. Each format needs its own fast path.
/// Decodes `data` to RGBA, applying EXIF orientation and optional resize,
/// and writes the result bottom-to-top directly into `out` — no intermediate
/// flip buffer or Vec allocation.
pub fn decode(data: &[u8], target_w: u32, target_h: u32, out: &mut [u8]) -> Result<(), String> {
    let orientation = exif_orientation(data);

    let img = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;

    let img = apply_orientation(img, orientation);

    let img = if target_w > 0 && target_h > 0
        && (img.width() != target_w || img.height() != target_h)
    {
        img.resize_exact(target_w, target_h, imageops::FilterType::Triangle)
    } else {
        img
    };

    // Flip rows directly into the output (OpenGL bottom-to-top convention).
    let img = img.into_rgba8();
    let w = img.width() as usize;
    let h = img.height() as usize;
    let row_bytes = w * 4;
    let src = img.as_raw();
    for row in 0..h {
        let src_row = &src[(h - 1 - row) * row_bytes..(h - row) * row_bytes];
        out[row * row_bytes..(row + 1) * row_bytes].copy_from_slice(src_row);
    }

    Ok(())
}

pub fn encode(rgba: &[u8], width: u32, height: u32, format: u32) -> Result<Vec<u8>, String> {
    let img = RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| "invalid image dimensions or buffer size".to_string())?;
    let dyn_img = DynamicImage::ImageRgba8(img);

    let fmt = match format {
        0 => ImageFormat::Png,
        1 => ImageFormat::Jpeg,
        _ => return Err(format!("unknown image format code: {format}")),
    };

    let mut buf = Cursor::new(Vec::new());
    dyn_img.write_to(&mut buf, fmt).map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// Returns the EXIF orientation tag value (1–8), or 1 (no-op) if absent or unreadable.
/// Cheap: reads only the EXIF segment from the file header.
pub(crate) fn exif_orientation(data: &[u8]) -> u32 {
    let mut cursor = Cursor::new(data);
    exif::Reader::new()
        .read_from_container(&mut cursor)
        .ok()
        .and_then(|exif| {
            exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
        })
        .unwrap_or(1)
}

/// Applies the EXIF orientation transform so the returned image is upright.
/// Orientations 5–8 transpose dimensions (w and h are swapped after this call).
fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.fliph().rotate270(), // transpose: mirror across main diagonal
        6 => img.rotate90(),
        7 => img.fliph().rotate90(),  // transverse: mirror across anti-diagonal
        8 => img.rotate270(),
        _ => img,
    }
}
