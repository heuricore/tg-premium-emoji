//! Static webp emoji, decoded by image-webp. An animated webp yields its first frame.

use std::io::Cursor;

use image_webp::{DecodingError, WebPDecoder};

use super::{Rgba, check_source_size};
use crate::error::Error;

pub fn decode(bytes: &[u8]) -> Result<Rgba, Error> {
    let invalid = |e: DecodingError| Error::Runtime(format!("invalid webp: {e}"));
    let mut decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(invalid)?;
    let (w, h) = decoder.dimensions();
    check_source_size("webp", w, h)?;
    let channels = if decoder.has_alpha() { 4 } else { 3 };
    let mut buf = vec![0; w as usize * h as usize * channels];
    decoder.read_image(&mut buf).map_err(invalid)?;
    let px = if channels == 4 {
        buf
    } else {
        buf.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect()
    };
    Ok(Rgba { w, h, px })
}
