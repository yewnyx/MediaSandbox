mod animation;
mod attrs;
mod audio;
mod img;

use std::alloc::Layout;

// ── Memory management ────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    if size == 0 {
        return 0;
    }
    let layout = Layout::from_size_align(size as usize, 8).expect("bad layout");
    unsafe { std::alloc::alloc(layout) as u32 }
}

#[no_mangle]
pub extern "C" fn dealloc(ptr: u32, size: u32) {
    if ptr == 0 || size == 0 {
        return;
    }
    let layout = Layout::from_size_align(size as usize, 8).expect("bad layout");
    unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
}

// ── Attribute query ──────────────────────────────────────────────────────────

/// Writes an `AttrResult` (48 bytes) to `out_ptr`.
/// Host must `alloc(48)` before calling and `dealloc(out_ptr, 48)` after reading.
#[no_mangle]
pub extern "C" fn query_attributes(data_ptr: u32, data_len: u32, out_ptr: u32) -> i32 {
    let data = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };
    let result = attrs::query(data);
    unsafe { *(out_ptr as *mut attrs::AttrResult) = result };
    if result.error_code == 0 { 0 } else { -(result.error_code as i32) }
}

// ── Image decode / encode ────────────────────────────────────────────────────

/// Decodes image bytes to RGBA. Host pre-allocates `out_len` = `target_w * target_h * 4`.
/// Pass target_w/target_h == 0 to decode at the image's native resolution.
/// Non-zero target dimensions resize the output (full decode then resize; see img::decode TODO).
#[no_mangle]
pub extern "C" fn decode_image(
    data_ptr: u32, data_len: u32,
    out_ptr: u32, out_len: u32,
    target_w: u32, target_h: u32,
) -> i32 {
    let data = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };
    let out = unsafe { std::slice::from_raw_parts_mut(out_ptr as *mut u8, out_len as usize) };

    match img::decode(data, target_w, target_h) {
        Ok(rgba) => {
            let n = rgba.len().min(out.len());
            out[..n].copy_from_slice(&rgba[..n]);
            0
        }
        Err(_) => -1,
    }
}

/// Encodes RGBA to the requested format.
/// WASM allocates the output internally; host reads `*out_ptr_ptr` and `*out_len_ptr`,
/// copies bytes out, then calls `dealloc(*out_ptr_ptr, *out_len_ptr)`.
/// format: 0=PNG, 1=JPEG
#[no_mangle]
pub extern "C" fn encode_image(
    rgba_ptr: u32,
    width: u32,
    height: u32,
    format: u32,
    out_ptr_ptr: u32,
    out_len_ptr: u32,
) -> i32 {
    let rgba =
        unsafe { std::slice::from_raw_parts(rgba_ptr as *const u8, (width * height * 4) as usize) };

    match img::encode(rgba, width, height, format) {
        Ok(encoded) => {
            let len = encoded.len() as u32;
            let dst = alloc(len);
            unsafe {
                std::ptr::copy_nonoverlapping(encoded.as_ptr(), dst as *mut u8, len as usize);
                *(out_ptr_ptr as *mut u32) = dst;
                *(out_len_ptr as *mut u32) = len;
            }
            0
        }
        Err(_) => -1,
    }
}

// ── Animation decode ─────────────────────────────────────────────────────────

/// Decodes GIF or WebP animation.
/// Host pre-allocates `out_len` = `4 + frame_count*4 + target_w*target_h*4*frame_count`.
/// Pass target_w/target_h == 0 to decode at native resolution.
///
/// Output layout:
///   [frame_count: u32 LE]
///   [delay_ms_0: u32 LE] … [delay_ms_N-1: u32 LE]
///   [frame_0_rgba: W×H×4] … [frame_N-1_rgba: W×H×4]
#[no_mangle]
pub extern "C" fn decode_animation(
    data_ptr: u32, data_len: u32,
    out_ptr: u32, out_len: u32,
    target_w: u32, target_h: u32,
) -> i32 {
    let data = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };
    let out = unsafe { std::slice::from_raw_parts_mut(out_ptr as *mut u8, out_len as usize) };

    let frames = match animation::decode(data, target_w, target_h) {
        Ok(f) => f,
        Err(_) => return -1,
    };

    let n = frames.len() as u32;
    let mut offset = 0usize;

    if offset + 4 > out.len() { return -2; }
    out[offset..offset + 4].copy_from_slice(&n.to_le_bytes());
    offset += 4;

    for (delay_ms, _) in &frames {
        if offset + 4 > out.len() { return -2; }
        out[offset..offset + 4].copy_from_slice(&delay_ms.to_le_bytes());
        offset += 4;
    }

    for (_, img) in &frames {
        let raw = img.as_raw();
        if offset + raw.len() > out.len() { return -2; }
        out[offset..offset + raw.len()].copy_from_slice(raw);
        offset += raw.len();
    }

    0
}

// ── Audio decode ─────────────────────────────────────────────────────────────

/// Decodes audio to interleaved f32 PCM.
/// WASM allocates the sample buffer internally; host reads:
///   *out_ptr_ptr  → pointer to f32 sample data
///   *out_len_ptr  → byte count (sample_count × 4)
///   *sr_ptr       → sample rate (u32)
///   *ch_ptr       → channel count (u32)
/// Host must call `dealloc(*out_ptr_ptr, *out_len_ptr)` after copying.
#[no_mangle]
pub extern "C" fn decode_audio(
    data_ptr: u32,
    data_len: u32,
    out_ptr_ptr: u32,
    out_len_ptr: u32,
    sr_ptr: u32,
    ch_ptr: u32,
) -> i32 {
    let data = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };

    match audio::decode(data) {
        Ok((samples, sample_rate, channels)) => {
            let byte_len = (samples.len() * 4) as u32;
            let dst = alloc(byte_len);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    samples.as_ptr() as *const u8,
                    dst as *mut u8,
                    byte_len as usize,
                );
                *(out_ptr_ptr as *mut u32) = dst;
                *(out_len_ptr as *mut u32) = byte_len;
                *(sr_ptr as *mut u32) = sample_rate;
                *(ch_ptr as *mut u32) = channels;
            }
            0
        }
        Err(_) => -1,
    }
}
