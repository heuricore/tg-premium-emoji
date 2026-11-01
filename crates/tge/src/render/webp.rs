//! Static webp emoji, decoded by image-webp. Animated webp is refused: a static emoji is one image.

use std::io::Cursor;

use image_webp::{DecodingError, WebPDecoder};

use super::{Rgba, check_source_size};
use crate::error::Error;

pub fn decode(bytes: &[u8]) -> Result<Rgba, Error> {
    let invalid = |e: DecodingError| Error::Runtime(format!("invalid webp: {e}"));
    let mut decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(invalid)?;
    let (w, h) = decoder.dimensions();
    check_source_size("webp", w, h)?;
    if decoder.is_animated() {
        return Err(animated());
    }
    check_image_chunk(bytes, w, h)?;
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

fn animated() -> Error {
    Error::Runtime("animated webp is not supported for a static emoji".into())
}

/// image-webp sizes the output from the canvas (the VP8X chunk, when present) but decodes a `VP8 `
/// chunk at the size its own frame header declares and compares the two only afterwards. So the
/// first image chunk, the one image-webp decodes, must declare the canvas size before anything is
/// decoded. A file with any animation frame is refused.
fn check_image_chunk(bytes: &[u8], w: u32, h: u32) -> Result<(), Error> {
    let mut image = None;
    // Chunks start after "RIFF", the file size and "WEBP"; each is a fourcc, a little-endian
    // payload size and the payload, padded to an even length.
    let mut pos = 12;
    while let Some(&[f0, f1, f2, f3, s0, s1, s2, s3]) =
        bytes.get(pos..).and_then(<[u8]>::first_chunk)
    {
        let size = u32::from_le_bytes([s0, s1, s2, s3]) as usize;
        let payload = &bytes[pos + 8..];
        let payload = &payload[..size.min(payload.len())];
        match &[f0, f1, f2, f3] {
            b"ANMF" => return Err(animated()),
            b"VP8 " if image.is_none() => image = Some(vp8_size(payload)?),
            b"VP8L" if image.is_none() => image = Some(vp8l_size(payload)?),
            _ => {}
        }
        let next = size
            .checked_add(size % 2)
            .and_then(|padded| padded.checked_add(pos + 8));
        match next {
            Some(next) => pos = next,
            None => break,
        }
    }
    match image {
        Some(size) if size == (w, h) => Ok(()),
        Some((iw, ih)) => Err(Error::Runtime(format!(
            "webp image is {iw}x{ih}, its canvas {w}x{h}"
        ))),
        None => Err(Error::Runtime("webp has no image chunk".into())),
    }
}

fn truncated() -> Error {
    Error::Runtime("invalid webp: truncated image chunk".into())
}

/// The size in a VP8 key frame header: 14 bits each, after the frame tag and start code.
fn vp8_size(payload: &[u8]) -> Result<(u32, u32), Error> {
    let Some(&[w0, w1, h0, h1]) = payload.get(6..10) else {
        return Err(truncated());
    };
    let side = |lo, hi| u32::from(u16::from_le_bytes([lo, hi]) & 0x3FFF);
    Ok((side(w0, w1), side(h0, h1)))
}

/// The size in a VP8L header: 14 bits each, minus one, after the signature byte.
fn vp8l_size(payload: &[u8]) -> Result<(u32, u32), Error> {
    let bits = payload
        .get(1..5)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(truncated)?;
    Ok(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
}
