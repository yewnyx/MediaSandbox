use std::io::Cursor;
use image::{AnimationDecoder, RgbaImage};

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
pub fn decode(data: &[u8]) -> Result<Vec<(u32, RgbaImage)>, String> {
    decode_frames(data)
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
