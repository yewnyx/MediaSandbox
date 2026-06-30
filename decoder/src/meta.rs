use std::io::Cursor;

use exif::{Reader, Tag};
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

// ── JSON helpers ──────────────────────────────────────────────────────────────

/// Escapes a string for embedding as a JSON string value (without surrounding quotes).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                write!(out, "\\u{:04x}", c as u32).unwrap();
            }
            c    => out.push(c),
        }
    }
    out
}

// ── XMP extraction ────────────────────────────────────────────────────────────

/// Returns the raw XMP XML from a JPEG file.
/// Uses zune-jpeg's header parser — cheap (no pixel decode).
fn extract_jpeg_xmp(data: &[u8]) -> Option<Vec<u8>> {
    if !data.starts_with(b"\xFF\xD8") { return None; }
    let mut decoder = JpegDecoder::new_with_options(
        Cursor::new(data),
        DecoderOptions::default(),
    );
    decoder.decode_headers().ok()?;
    decoder.xmp().cloned()
}

/// Returns the raw XMP XML from a PNG file by scanning iTXt chunks.
fn extract_png_xmp(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 8 || !data.starts_with(b"\x89PNG\r\n\x1a\n") { return None; }
    let mut pos = 8usize;
    while pos + 12 <= data.len() {
        let chunk_len = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        let chunk_type = &data[pos+4..pos+8];
        if pos + 8 + chunk_len > data.len() { break; }
        let chunk_data = &data[pos+8..pos+8+chunk_len];
        if chunk_type == b"IEND" { break; }
        if chunk_type == b"iTXt" {
            if let Some(kw_null) = chunk_data.iter().position(|&b| b == 0) {
                if &chunk_data[..kw_null] == b"XML:com.adobe.xmp" {
                    let rest = &chunk_data[kw_null + 1..];
                    // rest[0]=compression_flag, rest[1]=compression_method,
                    // then null-terminated language_tag and translated_keyword, then text
                    if rest.len() < 2 { break; }
                    let compression_flag = rest[0];
                    let mut offset = 2usize;
                    let lang_end = rest[offset..].iter().position(|&b| b == 0)? + offset + 1;
                    offset = lang_end;
                    let tkey_end = rest[offset..].iter().position(|&b| b == 0)? + offset + 1;
                    offset = tkey_end;
                    if compression_flag == 0 {
                        return Some(rest[offset..].to_vec());
                    }
                    // Compressed iTXt: would need deflate; skip for now.
                }
            }
        }
        pos += 12 + chunk_len; // 4 (len) + 4 (type) + chunk_len (data) + 4 (crc)
    }
    None
}

// ── EXIF extraction ───────────────────────────────────────────────────────────

/// Curated EXIF tags surfaced in the metadata JSON.
/// GPS tags live in the GPS sub-IFD; kamadak-exif's fields() iterator covers all IFDs.
const INTERESTING_TAGS: &[(Tag, &str)] = &[
    (Tag::Make,             "Make"),
    (Tag::Model,            "Model"),
    (Tag::Software,         "Software"),
    (Tag::DateTime,         "DateTime"),
    (Tag::DateTimeOriginal, "DateTimeOriginal"),
    (Tag::ImageDescription, "ImageDescription"),
    (Tag::Orientation,      "Orientation"),
    (Tag::GPSLatitude,      "GPSLatitude"),
    (Tag::GPSLatitudeRef,   "GPSLatitudeRef"),
    (Tag::GPSLongitude,     "GPSLongitude"),
    (Tag::GPSLongitudeRef,  "GPSLongitudeRef"),
    (Tag::GPSAltitude,      "GPSAltitude"),
    (Tag::GPSAltitudeRef,   "GPSAltitudeRef"),
];

/// Reads a curated set of EXIF fields from any container supported by kamadak-exif
/// (JPEG, TIFF; gracefully returns empty for other formats).
fn extract_exif_fields(data: &[u8]) -> Vec<(String, String)> {
    let mut cursor = Cursor::new(data);
    let exif = match Reader::new().read_from_container(&mut cursor) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut fields = Vec::new();
    for field in exif.fields() {
        for &(tag, name) in INTERESTING_TAGS {
            if field.tag == tag {
                fields.push((name.to_string(), field.display_value().to_string()));
                break;
            }
        }
    }
    fields
}

// ── Metadata stripping ────────────────────────────────────────────────────────

/// Removes metadata segments from a JPEG file without re-encoding pixel data.
/// Strips APP1–APP15 (EXIF, XMP, ICC, IPTC, …) and COM (comment) markers.
/// Keeps APP0 (JFIF header) and all structural markers (SOF, DHT, DQT, SOS, …).
/// On encountering the first SOS marker, copies from SOS to end-of-file verbatim —
/// this is correct for both sequential and progressive JPEGs because APP markers
/// can only appear before the first scan.
fn strip_jpeg(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[..2]); // SOI
    let mut pos = 2usize;
    while pos + 2 <= data.len() {
        if data[pos] != 0xFF { break; }
        let marker = data[pos + 1];
        match marker {
            0xD8 => { pos += 2; } // nested SOI (unusual)
            0xD9 => { out.extend_from_slice(&data[pos..pos + 2]); break; } // EOI
            0xDA => { out.extend_from_slice(&data[pos..]); break; } // SOS — copy to EOF
            0xD0..=0xD7 => { // RST0–RST7: no length field
                out.extend_from_slice(&data[pos..pos + 2]);
                pos += 2;
            }
            _ => {
                if pos + 4 > data.len() { break; }
                let seg_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
                if pos + 2 + seg_len > data.len() { break; }
                let keep = !matches!(marker,
                    0xE1..=0xEF | // APP1–APP15 (EXIF, XMP, ICC, etc.)
                    0xFE          // COM (comment)
                );
                if keep {
                    out.extend_from_slice(&data[pos..pos + 2 + seg_len]);
                }
                pos += 2 + seg_len;
            }
        }
    }
    out
}

/// Removes metadata chunks from a PNG file.
/// Drops: tEXt, zTXt, iTXt (XMP), eXIf. Keeps all structural chunks.
fn strip_png(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[..8]); // PNG signature
    let mut pos = 8usize;
    while pos + 12 <= data.len() {
        let chunk_len = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        if pos + 12 + chunk_len > data.len() { break; }
        let chunk_type = &data[pos+4..pos+8];
        let is_iend = chunk_type == b"IEND";
        let strip = matches!(chunk_type, b"tEXt" | b"zTXt" | b"iTXt" | b"eXIf");
        if !strip {
            out.extend_from_slice(&data[pos..pos + 12 + chunk_len]);
        }
        pos += 12 + chunk_len;
        if is_iend { break; }
    }
    out
}

/// Removes EXIF and XMP chunks from a WebP RIFF file and fixes up the file-size field.
fn strip_webp(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&[0u8; 4]); // size placeholder — patched at the end
    out.extend_from_slice(b"WEBP");
    let mut pos = 12usize;
    while pos + 8 <= data.len() {
        let chunk_type = &data[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([data[pos+4], data[pos+5], data[pos+6], data[pos+7]]) as usize;
        let padded = chunk_size + (chunk_size & 1); // RIFF chunks are padded to even byte boundary
        if pos + 8 + padded > data.len() { break; }
        let strip = matches!(chunk_type, b"EXIF" | b"XMP ");
        if !strip {
            out.extend_from_slice(&data[pos..pos + 8 + padded]);
        }
        pos += 8 + padded;
    }
    let riff_size = (out.len() as u32).wrapping_sub(8);
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());
    out
}

/// Strips all EXIF, XMP, and associated metadata from a JPEG, PNG, or WebP file
/// without re-encoding pixel data.
///
/// Returns `Some(stripped_bytes)` for supported formats.
/// Returns `None` for formats where in-place stripping is not implemented (TIFF, GIF,
/// BMP, …); callers should fall back to a decode→encode round-trip if stripping is
/// required for those.
pub fn strip_metadata(data: &[u8]) -> Option<Vec<u8>> {
    if data.starts_with(b"\xFF\xD8") {
        Some(strip_jpeg(data))
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(strip_png(data))
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(strip_webp(data))
    } else {
        None
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Returns a UTF-8 JSON object with up to two keys:
///   "exif"       — object mapping curated EXIF tag names to display strings
///   "xmp_packet" — raw XMP XML string extracted from the file
/// Either key may be absent if the format has no such data. Returns "{}" when no
/// metadata is found.
pub fn query_metadata(data: &[u8]) -> String {
    let exif_fields = extract_exif_fields(data);
    let xmp_bytes: Option<Vec<u8>> = extract_jpeg_xmp(data)
        .or_else(|| extract_png_xmp(data));
    let xmp_str: Option<&str> = xmp_bytes
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok());

    let mut json = String::new();
    json.push('{');
    let mut sep = false;

    if !exif_fields.is_empty() {
        json.push_str("\"exif\":{");
        for (i, (k, v)) in exif_fields.iter().enumerate() {
            if i > 0 { json.push(','); }
            json.push('"');
            json.push_str(&json_escape(k));
            json.push_str("\":\"");
            json.push_str(&json_escape(v));
            json.push('"');
        }
        json.push('}');
        sep = true;
    }

    if let Some(xmp) = xmp_str {
        if sep { json.push(','); }
        json.push_str("\"xmp_packet\":\"");
        json.push_str(&json_escape(xmp));
        json.push('"');
    }

    json.push('}');
    json
}
