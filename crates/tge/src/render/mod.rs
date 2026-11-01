//! Decoding emoji sources into straight RGBA frames.

pub mod tgs;

use crate::error::Error;
use crate::model::pack::Format;

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

/// An opened emoji source. Holds native decoder state, so it is neither `Send` nor `Sync`.
#[derive(Debug)]
pub enum Media {
    Tgs(tgs::Animation),
}

impl Media {
    pub fn open(format: Format, bytes: &[u8]) -> Result<Media, Error> {
        match format {
            Format::Tgs => Ok(Media::Tgs(tgs::Animation::open(bytes)?)),
            Format::Static | Format::Webm => Err(Error::Runtime(format!(
                "decoding {format:?} sources is not supported yet"
            ))),
        }
    }

    pub fn frame_count(&self) -> usize {
        match self {
            Media::Tgs(anim) => anim.frame_count(),
        }
    }

    /// Frame `index` (0-based, below `frame_count`) as a `size`×`size` image.
    pub fn frame(&mut self, index: usize, size: u32) -> Result<Rgba, Error> {
        match self {
            Media::Tgs(anim) => anim.render(index, size),
        }
    }
}
