//! Decoding emoji sources into straight RGBA frames.

pub mod frames;
pub mod scale;
pub mod tgs;
pub mod webm;
pub mod webp;

use crate::error::Error;
use crate::model::pack::Format;

/// Largest width or height accepted from a raster source; Telegram custom emoji are 100×100.
pub const MAX_SOURCE_SIDE: u32 = 1024;

/// Straight (non-premultiplied) RGBA8 image, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Rgba {
    /// A fully transparent image.
    pub fn new(w: u32, h: u32) -> Rgba {
        Rgba {
            w,
            h,
            px: vec![0; w as usize * h as usize * 4],
        }
    }
}

/// An opened emoji source. The tgs variant holds native decoder state, so `Media` is neither
/// `Send` nor `Sync`.
#[derive(Debug)]
pub enum Media {
    Tgs(tgs::Animation),
    /// Every frame, decoded on open.
    Webm(Vec<webm::YuvaFrame>),
    Static(Rgba),
}

impl Media {
    pub fn open(format: Format, bytes: &[u8]) -> Result<Media, Error> {
        Ok(match format {
            Format::Tgs => Media::Tgs(tgs::Animation::open(bytes)?),
            Format::Webm => Media::Webm(webm::decode_yuva(bytes)?),
            Format::Static => Media::Static(webp::decode(bytes)?),
        })
    }

    pub fn frame_count(&self) -> usize {
        match self {
            Media::Tgs(anim) => anim.frame_count(),
            Media::Webm(frames) => frames.len(),
            Media::Static(_) => 1,
        }
    }

    /// Frame `index` (0-based, below `frame_count`) as a `size`×`size` image. tgs is rendered at
    /// that size; raster formats are fitted inside it with their aspect ratio kept, centred on a
    /// transparent canvas.
    pub fn frame(&mut self, index: usize, size: u32) -> Result<Rgba, Error> {
        let count = self.frame_count();
        match self {
            Media::Tgs(anim) => anim.render(index, size),
            _ if size == 0 || size > tgs::MAX_SIZE => Err(Error::Runtime(format!(
                "render size {size} is outside 1..={}",
                tgs::MAX_SIZE
            ))),
            _ if index >= count => Err(Error::Runtime(format!(
                "frame {index} is out of range, the source has {count} frame(s)"
            ))),
            Media::Webm(frames) => Ok(scale::fit_square(&webm::to_rgba(&frames[index]), size)),
            Media::Static(img) => Ok(scale::fit_square(img, size)),
        }
    }
}

/// Rejects an empty raster or one larger than `MAX_SOURCE_SIDE`; decoders call it before they
/// allocate pixels wherever the format declares its size up front.
fn check_source_size(kind: &str, w: u32, h: u32) -> Result<(), Error> {
    if w == 0 || h == 0 || w > MAX_SOURCE_SIDE || h > MAX_SOURCE_SIDE {
        return Err(Error::Runtime(format!(
            "{kind} frame is {w}x{h}, outside 1x1 to {MAX_SOURCE_SIDE}x{MAX_SOURCE_SIDE}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(w: u32, h: u32) -> Rgba {
        Rgba {
            w,
            h,
            px: [10, 20, 30, 255].repeat(w as usize * h as usize),
        }
    }

    #[test]
    fn square_raster_is_resized_to_size() {
        let mut media = Media::Static(opaque(100, 100));
        let out = media.frame(0, 64).unwrap();
        assert_eq!((out.w, out.h), (64, 64));
        assert!(
            out.px
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [10, 20, 30, 255])
        );
    }

    #[test]
    fn non_square_raster_is_centred_with_transparent_bands() {
        let mut media = Media::Static(opaque(100, 50));
        let out = media.frame(0, 64).unwrap();
        assert_eq!((out.w, out.h), (64, 64));
        let alpha = |x: usize, y: usize| out.px[(y * 64 + x) * 4 + 3];
        assert_eq!(alpha(32, 0), 0);
        assert_eq!(alpha(32, 63), 0);
        assert_eq!(alpha(0, 32), 255);
        assert_eq!(alpha(63, 32), 255);
        assert_eq!(alpha(32, 32), 255);
    }

    #[test]
    fn raster_size_bounds_are_enforced() {
        let mut media = Media::Static(opaque(4, 4));
        assert!(media.frame(0, 0).is_err());
        assert!(media.frame(0, tgs::MAX_SIZE + 1).is_err());
        assert!(media.frame(1, 8).is_err());
    }
}
