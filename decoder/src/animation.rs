use std::io::Cursor;
use image::{imageops, AnimationDecoder, DynamicImage, RgbaImage};

fn is_gif(data: &[u8]) -> bool {
    data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")
}

fn is_webp(data: &[u8]) -> bool {
    data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP"
}

// ── Fast container-level queries (no pixel decode) ────────────────────────────

fn skip_gif_sub_blocks(data: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        if pos >= data.len() { return None; }
        let size = data[pos] as usize;
        pos += 1;
        if size == 0 { return Some(pos); }
        pos += size;
    }
}

/// Parses the GIF block structure to extract (width, height, frame_count)
/// without decompressing any pixel data.
fn gif_fast_query(data: &[u8]) -> Option<(u32, u32, u32)> {
    if data.len() < 13 { return None; }
    if !is_gif(data) { return None; }

    let w = u16::from_le_bytes([data[6], data[7]]) as u32;
    let h = u16::from_le_bytes([data[8], data[9]]) as u32;
    let packed    = data[10];
    let has_gct   = (packed & 0x80) != 0;
    let gct_bytes = if has_gct { 3usize << ((packed & 0x07) as usize + 1) } else { 0 };

    let mut pos: usize = 13 + gct_bytes;
    let mut frame_count: u32 = 0;

    while pos < data.len() {
        match data[pos] {
            0x3B => break, // Trailer — end of file
            0x21 => {
                // Extension introducer + label, then sub-blocks
                pos += 2;
                pos = skip_gif_sub_blocks(data, pos)?;
            }
            0x2C => {
                // Image Descriptor — one frame
                frame_count += 1;
                if pos + 10 > data.len() { break; }
                let local_packed = data[pos + 9];
                let has_lct   = (local_packed & 0x80) != 0;
                let lct_bytes = if has_lct { 3usize << ((local_packed & 0x07) as usize + 1) } else { 0 };
                pos += 10 + lct_bytes; // skip descriptor + local color table
                if pos >= data.len() { break; }
                pos += 1; // LZW minimum code size byte
                pos = skip_gif_sub_blocks(data, pos)?; // skip compressed image data
            }
            _ => break, // Unknown block — stop (don't corrupt the count)
        }
    }

    if frame_count == 0 || w == 0 || h == 0 { return None; }
    Some((w, h, frame_count))
}

/// Scans RIFF/WebP chunks to extract (width, height, frame_count) for
/// animated WebP without decoding any pixel data.
fn webp_fast_query(data: &[u8]) -> Option<(u32, u32, u32)> {
    if data.len() < 30 { return None; }
    if !is_webp(data) { return None; }
    if &data[12..16] != b"VP8X" { return None; }
    if (data[20] & 0x02) == 0 { return None; } // animation flag not set

    // Canvas size: 24-bit LE, stored as (value - 1)
    let w = u32::from_le_bytes([data[24], data[25], data[26], 0]) + 1;
    let h = u32::from_le_bytes([data[27], data[28], data[29], 0]) + 1;

    let riff_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let file_end  = (8 + riff_size).min(data.len());

    let mut pos: usize = 12; // first chunk after "RIFF????WEBP"
    let mut frame_count: u32 = 0;

    while pos + 8 <= file_end {
        if &data[pos..pos + 4] == b"ANMF" {
            frame_count += 1;
        }
        let chunk_size = u32::from_le_bytes([
            data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7],
        ]) as usize;
        // RIFF chunks are padded to even offsets
        pos += 8 + chunk_size + (chunk_size & 1);
    }

    if frame_count == 0 || w == 0 || h == 0 { return None; }
    Some((w, h, frame_count))
}

// Returns (width, height, frame_count).
// Uses fast container-level parsing; falls back to full decode only for malformed input.
pub fn query(data: &[u8]) -> Result<(u32, u32, u32), String> {
    if let Some(r) = gif_fast_query(data).or_else(|| webp_fast_query(data)) {
        return Ok(r);
    }
    // Fallback: full decode (handles corrupt or non-standard containers).
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
///
/// The `frames` iterator borrows from the WASM allocation at `data_ptr`.
/// `ManuallyDrop` ensures it is torn down before `data_ptr` is freed in `Drop`.
pub struct AnimHandle {
    frames:   std::mem::ManuallyDrop<image::Frames<'static>>,
    target_w: u32,
    target_h: u32,
    data_ptr: u32,
    data_len: u32,
}

impl Drop for AnimHandle {
    fn drop(&mut self) {
        // Drop the iterator first — it holds a borrow into data_ptr's allocation.
        unsafe { std::mem::ManuallyDrop::drop(&mut self.frames); }
        if self.data_ptr != 0 {
            crate::dealloc(self.data_ptr, self.data_len);
        }
    }
}

/// Creates a streaming decoder for `data_ptr`.
///
/// Ownership of the allocation is transferred: `AnimHandle::drop` will call
/// `crate::dealloc(data_ptr, data_len)`. The caller must NOT dealloc `data_ptr`
/// after a successful return; on error, the caller MUST dealloc it.
///
/// No copy of the animation data is made — the decoder reads directly from
/// the existing WASM allocation. WASM linear memory is never unmapped, so the
/// pointer remains valid for the full lifetime of the returned handle.
pub fn open(data_ptr: u32, data_len: u32, target_w: u32, target_h: u32) -> Result<Box<AnimHandle>, String> {
    // Safety: data_ptr is a live WASM allocation (not yet dealloc'd) that will
    // stay valid until AnimHandle::drop() calls crate::dealloc. WASM linear
    // memory is never remapped or freed by the runtime, so the 'static cast is sound.
    let data: &'static [u8] = unsafe {
        std::slice::from_raw_parts(data_ptr as *const u8, data_len as usize)
    };

    let frames: image::Frames<'static> = if is_gif(data) {
        use image::codecs::gif::GifDecoder;
        GifDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?.into_frames()
    } else if is_webp(data) {
        use image::codecs::webp::WebPDecoder;
        WebPDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?.into_frames()
    } else {
        return Err("unsupported animation format".into());
    };

    Ok(Box::new(AnimHandle {
        frames:   std::mem::ManuallyDrop::new(frames),
        target_w,
        target_h,
        data_ptr,
        data_len,
    }))
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

    let img = frame.into_buffer(); // RgbaImage — no extra copy; just takes the buffer
    let (tw, th) = (handle.target_w, handle.target_h);

    let needs_resize = tw > 0 && th > 0 && (img.width() != tw || img.height() != th);
    let img: RgbaImage = if needs_resize {
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
        let src_row = &src[(h - 1 - row) * row_bytes..(h - row) * row_bytes];
        out_rgba[row * row_bytes..(row + 1) * row_bytes].copy_from_slice(src_row);
    }

    Some(Ok(delay_ms))
}
