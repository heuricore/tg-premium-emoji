//! Hidden `tge render`: frames of one source as straight-RGBA PNG files, so the release pipeline
//! can compare every platform's rendering with Linux x64.

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::error::Error;
use crate::model::pack::Format;
use crate::output::Ctx;
use crate::render::frames::{SAMPLES, sample_positions};
use crate::render::sheet::{ICON, write_png};
use crate::render::{Media, Rgba, webm};

pub struct Args {
    pub file: PathBuf,
    pub format: Format,
    pub out: PathBuf,
    pub size: Option<u32>,
    pub frames: Option<Vec<usize>>,
}

pub fn run(ctx: &Ctx, args: Args) -> Result<(), Error> {
    let bytes = fs::read(&args.file)
        .map_err(|e| Error::Runtime(format!("cannot read {}: {e}", args.file.display())))?;
    let mut media = Media::open(args.format, &bytes)?;
    let total = media.frame_count();
    let frames = match args.frames {
        Some(frames) => frames,
        None if args.format == Format::Tgs => sample_positions(total, SAMPLES),
        None => (0..total).collect(),
    };
    if let Some(bad) = frames.iter().find(|&&i| i >= total) {
        return Err(Error::Runtime(format!(
            "frame {bad} is out of range, the source has {total} frame(s)"
        )));
    }
    fs::create_dir_all(&args.out)?;
    for &index in &frames {
        let img = match args.size {
            Some(size) => media.frame(index, size)?,
            None => native_frame(&mut media, index)?,
        };
        write_png(&args.out.join(format!("{index}.png")), &img)?;
    }
    ctx.emit(
        &format!(
            "rendered {} of {total} frame(s) into {}",
            frames.len(),
            args.out.display()
        ),
        json!({ "out": args.out, "frame_count": total, "frames": frames }),
    );
    Ok(())
}

/// Raster frames as decoded, with no scaling; tgs, which has no pixel size, at the preview icon
/// size.
fn native_frame(media: &mut Media, index: usize) -> Result<Rgba, Error> {
    match media {
        Media::Tgs(anim) => anim.render(index, ICON),
        Media::Webm(frames) => Ok(webm::to_rgba(&frames[index])),
        Media::Static(img) => Ok(img.clone()),
    }
}
