//! Decoding emoji sources into straight RGBA frames.

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

    /// Frame `index` (0-based, below `frame_count`). tgs is rendered at `size`×`size`; raster
    /// formats come at their native size.
    pub fn frame(&mut self, index: usize, size: u32) -> Result<Rgba, Error> {
        let count = self.frame_count();
        match self {
            Media::Tgs(anim) => anim.render(index, size),
            _ if index >= count => Err(Error::Runtime(format!(
                "frame {index} is out of range, the source has {count} frame(s)"
            ))),
            Media::Webm(frames) => Ok(webm::to_rgba(&frames[index])),
            Media::Static(img) => Ok(img.clone()),
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
