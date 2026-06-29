use std::io::Cursor;
use image::{ImageReader, DynamicImage, ImageFormat, RgbaImage};

pub fn decode(data: &[u8]) -> Result<Vec<u8>, String> {
    let img = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .into_rgba8();
    Ok(img.into_raw())
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
