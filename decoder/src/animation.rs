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
