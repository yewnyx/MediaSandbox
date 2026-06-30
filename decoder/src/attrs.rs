use std::io::Cursor;

// Matches the C# MediaDecoderSandbox.AttrResultSize constant exactly (no padding).
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct AttrResult {
    pub duration_ms: u64,           // offset 0
    pub required_buffer_size: u64,  // offset 8
    pub media_type: u32,            // offset 16  (0=Unknown 1=Image 2=Animation 3=Audio)
    pub width: u32,                 // offset 20
    pub height: u32,                // offset 24
    pub frame_count: u32,           // offset 28  (1 for stills)
    pub sample_rate: u32,           // offset 32
    pub channel_count: u32,         // offset 36
    pub page_count: u32,            // offset 40  (reserved for future PDF use)
    pub error_code: u32,            // offset 44  (0 = ok)
    pub flags: u32,                 // offset 48  bit 0 = ALPHA_POSSIBLE
}
// Total: 56 bytes (52 bytes of fields + 4 bytes tail-padding for 8-byte struct alignment)

/// Bit 0 of `flags`: the format may carry an alpha channel.
/// When clear, the decoder guarantees fully-opaque output (JPEG, HDR, BMP).
pub const ALPHA_POSSIBLE: u32 = 1;

#[repr(u32)]
pub enum MediaKind {
    Unknown   = 0,
    Image     = 1,
    Animation = 2,
    Audio     = 3,
}

pub fn sniff(data: &[u8]) -> MediaKind {
    if data.len() < 8 {
        return MediaKind::Unknown;
    }
    // PNG
    if data.starts_with(b"\x89PNG\r\n\x1a\n") { return MediaKind::Image; }
    // JPEG
    if data.starts_with(b"\xFF\xD8\xFF") { return MediaKind::Image; }
    // GIF — always animation; single-frame GIF is handled as a 1-frame animation
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") { return MediaKind::Animation; }
    // BMP
    if data.starts_with(b"BM") { return MediaKind::Image; }
    // TIFF LE or BE
    if data.starts_with(b"II\x2A\x00") || data.starts_with(b"MM\x00\x2A") { return MediaKind::Image; }
    // Radiance HDR
    if data.starts_with(b"#?RADIANCE") || data.starts_with(b"#?RGBE") { return MediaKind::Image; }
    // QOI
    if data.starts_with(b"qoif") { return MediaKind::Image; }
    // FLAC
    if data.starts_with(b"fLaC") { return MediaKind::Audio; }
    // OGG (covers Vorbis, FLAC-in-OGG, Opus-in-OGG)
    if data.starts_with(b"OggS") { return MediaKind::Audio; }
    // MP3 with ID3 tag
    if data.starts_with(b"ID3") { return MediaKind::Audio; }
    // MP3 sync word (0xFFEx or 0xFFFx)
    if data[0] == 0xFF && (data[1] & 0xE0) == 0xE0 { return MediaKind::Audio; }
    // RIFF-based: WebP or WAV
    if data.starts_with(b"RIFF") && data.len() >= 12 {
        if &data[8..12] == b"WEBP" {
            // VP8X extended format with animation flag (bit 1 of flags byte at offset 20)
            // means animated WebP; everything else is a still image.
            let is_animated = data.len() >= 21
                && &data[12..16] == b"VP8X"
                && (data[20] & 0x02) != 0;
            return if is_animated { MediaKind::Animation } else { MediaKind::Image };
        }
        if &data[8..12] == b"WAVE" { return MediaKind::Audio; }
    }
    // AIFF / AIFC
    if data.starts_with(b"FORM") && data.len() >= 12 {
        if &data[8..12] == b"AIFF" || &data[8..12] == b"AIFC" { return MediaKind::Audio; }
    }
    MediaKind::Unknown
}

pub fn query(data: &[u8]) -> AttrResult {
    match sniff(data) {
        MediaKind::Image     => query_image(data),
        MediaKind::Animation => query_animation(data),
        MediaKind::Audio     => query_audio(data),
        MediaKind::Unknown   => AttrResult { error_code: 1, ..Default::default() },
    }
}

fn query_image(data: &[u8]) -> AttrResult {
    use image::ImageReader;

    // Determine orientation-corrected dimensions without a full decode.
    // Orientations 5–8 rotate 90/270° and therefore swap width and height.
    let orientation = crate::img::exif_orientation(data);
    let swaps_dims  = matches!(orientation, 5 | 6 | 7 | 8);

    // JPEG, HDR, and BMP have no alpha channel.
    let flags = if data.starts_with(b"\xFF\xD8\xFF")
        || data.starts_with(b"#?RADIANCE")
        || data.starts_with(b"#?RGBE")
        || data.starts_with(b"BM")
    {
        0
    } else {
        ALPHA_POSSIBLE
    };

    match ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
    {
        Some((raw_w, raw_h)) => {
            let (w, h) = if swaps_dims { (raw_h, raw_w) } else { (raw_w, raw_h) };
            AttrResult {
                media_type: MediaKind::Image as u32,
                width: w,
                height: h,
                frame_count: 1,
                required_buffer_size: (w as u64) * (h as u64) * 4,
                flags,
                ..Default::default()
            }
        }
        None => AttrResult { media_type: MediaKind::Image as u32, error_code: 2, ..Default::default() },
    }
}

fn query_animation(data: &[u8]) -> AttrResult {
    match crate::animation::query(data) {
        Ok((w, h, n)) => {
            let frame_bytes = (w as u64) * (h as u64) * 4;
            let header = 4 + (n as u64) * 4; // frame_count u32 + N×delay_ms u32
            AttrResult {
                media_type: MediaKind::Animation as u32,
                width: w,
                height: h,
                frame_count: n,
                required_buffer_size: header + frame_bytes * (n as u64),
                flags: ALPHA_POSSIBLE, // GIF and WebP both support alpha
                ..Default::default()
            }
        }
        Err(_) => AttrResult { media_type: MediaKind::Animation as u32, error_code: 3, ..Default::default() },
    }
}

fn query_audio(data: &[u8]) -> AttrResult {
    match crate::audio::query(data) {
        Ok((sample_rate, channel_count, duration_ms)) => AttrResult {
            media_type: MediaKind::Audio as u32,
            sample_rate,
            channel_count,
            duration_ms,
            // required_buffer_size intentionally left 0: audio uses WASM-internal alloc
            ..Default::default()
        },
        Err(_) => AttrResult { media_type: MediaKind::Audio as u32, error_code: 4, ..Default::default() },
    }
}
