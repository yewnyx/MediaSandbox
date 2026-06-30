use std::io::Cursor;
use image::{imageops, AnimationDecoder, DynamicImage, RgbaImage};

fn is_gif(data: &[u8]) -> bool {
    data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")
}

fn is_webp(data: &[u8]) -> bool {
    data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP"
}

// Returns (width, height, frame_count).
// Decodes all frames to count them; this also validates the input.
pub fn query(data: &[u8]) -> Result<(u32, u32, u32), String> {
    let frames = decode_frames(data)?;
    if frames.is_empty() {
        return Err("no frames decoded".into());
    }
    let (w, h) = (frames[0].1.width(), frames[0].1.height());
    Ok((w, h, frames.len() as u32))
}

// Returns Vec<(delay_ms, RgbaImage)>
// TODO(perf): decode frames at target size without full-resolution intermediates.
pub fn decode(data: &[u8], target_w: u32, target_h: u32) -> Result<Vec<(u32, RgbaImage)>, String> {
    let frames = decode_frames(data)?;

    // Unity's LoadRawTextureData expects bottom-to-top row order (OpenGL convention).
    // Flip every frame unconditionally. Resize only when a valid target is given.
    Ok(frames
        .into_iter()
        .map(|(delay, img)| {
            let needs_resize = target_w > 0 && target_h > 0
                && (img.width() != target_w || img.height() != target_h);
            let img = if needs_resize {
                DynamicImage::ImageRgba8(img)
                    .resize_exact(target_w, target_h, imageops::FilterType::Triangle)
                    .into_rgba8()
            } else {
                img
            };
            (delay, imageops::flip_vertical(&img))
        })
        .collect())
}

fn decode_frames(data: &[u8]) -> Result<Vec<(u32, RgbaImage)>, String> {
    if is_gif(data) {
        decode_gif(data)
    } else if is_webp(data) {
        decode_webp(data)
    } else {
        Err("not a supported animation format".into())
    }
}

fn collect_delay_and_image(
    frames: image::Frames<'_>,
) -> Result<Vec<(u32, RgbaImage)>, String> {
    let collected = frames.collect_frames().map_err(|e| e.to_string())?;
    Ok(collected
        .into_iter()
        .map(|f| {
            let (numer, denom) = f.delay().numer_denom_ms();
            let delay_ms = if denom == 0 { 100 } else { numer / denom };
            (delay_ms, f.into_buffer())
        })
        .collect())
}

fn decode_gif(data: &[u8]) -> Result<Vec<(u32, RgbaImage)>, String> {
    use image::codecs::gif::GifDecoder;
    let decoder = GifDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?;
    collect_delay_and_image(decoder.into_frames())
}

fn decode_webp(data: &[u8]) -> Result<Vec<(u32, RgbaImage)>, String> {
    use image::codecs::webp::WebPDecoder;
    let decoder = WebPDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?;
    collect_delay_and_image(decoder.into_frames())
}

// ── Streaming API ─────────────────────────────────────────────────────────────

/// Opaque decoder state held across `animation_next_frame` calls.
/// `Frames<'static>` is valid here because the iterator owns its data via
/// `GifDecoder<Cursor<Vec<u8>>>` / `WebPDecoder<Cursor<Vec<u8>>>`, both of which
/// are `'static` — no borrowed slices survive beyond construction.
pub struct AnimHandle {
    frames: image::Frames<'static>,
    target_w: u32,
    target_h: u32,
}

/// Creates a streaming decoder for `data`. Takes ownership of `data` (no host copy needed
/// after this returns). Returns `Ok(Box<AnimHandle>)` on success.
pub fn open(data: Vec<u8>, target_w: u32, target_h: u32) -> Result<Box<AnimHandle>, String> {
    // Check format before moving data into the cursor.
    let frames: image::Frames<'static> = if is_gif(&data) {
        use image::codecs::gif::GifDecoder;
        GifDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?.into_frames()
    } else if is_webp(&data) {
        use image::codecs::webp::WebPDecoder;
        WebPDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?.into_frames()
    } else {
        return Err("unsupported animation format".into());
    };
    Ok(Box::new(AnimHandle { frames, target_w, target_h }))
}

/// Decodes the next frame, writing flipped RGBA directly into `out_rgba`.
/// Returns `None` when the animation is exhausted, `Some(Ok(delay_ms))` on success.
/// `out_rgba` must be exactly `target_w * target_h * 4` bytes.
pub fn next_frame(handle: &mut AnimHandle, out_rgba: &mut [u8]) -> Option<Result<u32, String>> {
    let frame = match handle.frames.next()? {
        Ok(f)  => f,
        Err(e) => return Some(Err(e.to_string())),
    };

    let (numer, denom) = frame.delay().numer_denom_ms();
    let delay_ms = if denom == 0 { 100 } else { numer / denom };

    let img = frame.into_buffer(); // RgbaImage
    let (tw, th) = (handle.target_w, handle.target_h);

    let needs_resize = tw > 0 && th > 0 && (img.width() != tw || img.height() != th);
    let img = if needs_resize {
        DynamicImage::ImageRgba8(img)
            .resize_exact(tw, th, imageops::FilterType::Triangle)
            .into_rgba8()
    } else {
        img
    };

    // Flip rows directly into the output — avoids the intermediate buffer that
    // imageops::flip_vertical would allocate.
    let w = img.width() as usize;
    let h = img.height() as usize;
    let row_bytes = w * 4;
    let src = img.as_raw();
    for row in 0..h {
        let src_row = &src[(h - 1 - row) * row_bytes .. (h - row) * row_bytes];
        out_rgba[row * row_bytes .. (row + 1) * row_bytes].copy_from_slice(src_row);
    }

    Some(Ok(delay_ms))
}
