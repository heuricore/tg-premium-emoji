//! tgs (gzipped Lottie JSON) rendered by the vendored rlottie through its C API.

use std::ffi::{CString, c_char};
use std::io::Read;
use std::ptr::NonNull;
use std::sync::{Mutex, PoisonError};

use flate2::read::GzDecoder;

use super::Rgba;
use crate::error::Error;

/// Cap on the decompressed Lottie JSON.
pub const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;

/// Largest rendered side; keeps rlottie's `int` geometry far from overflow.
const MAX_SIZE: u32 = 4096;

/// Built without LOTTIE_THREAD_SUPPORT or LOTTIE_THREAD_SAFE, rlottie rasterizes every animation
/// through one process-wide outline and stroker (`RleTaskScheduler` in `vraster.cpp`), so two
/// animations must never be inside rlottie at the same time.
static RLOTTIE: Mutex<()> = Mutex::new(());

fn locked<T>(call: impl FnOnce() -> T) -> T {
    let _guard = RLOTTIE.lock().unwrap_or_else(PoisonError::into_inner);
    call()
}

#[repr(C)]
struct LottieAnimation {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn lottie_animation_from_data(
        data: *const c_char,
        key: *const c_char,
        resource_path: *const c_char,
    ) -> *mut LottieAnimation;
    fn lottie_animation_destroy(animation: *mut LottieAnimation);
    fn lottie_animation_get_size(
        animation: *const LottieAnimation,
        width: *mut usize,
        height: *mut usize,
    );
    fn lottie_animation_get_totalframe(animation: *const LottieAnimation) -> usize;
    fn lottie_animation_render(
        animation: *mut LottieAnimation,
        frame_num: usize,
        buffer: *mut u32,
        width: usize,
        height: usize,
        bytes_per_line: usize,
    );
}

/// A loaded animation. Owns the rlottie handle, which makes it neither `Send` nor `Sync`.
#[derive(Debug)]
pub struct Animation {
    raw: NonNull<LottieAnimation>,
    frames: usize,
}

impl Animation {
    pub fn open(tgs: &[u8]) -> Result<Animation, Error> {
        let json = CString::new(inflate(tgs)?)
            .map_err(|_| Error::Runtime("tgs JSON contains a NUL byte".into()))?;
        // SAFETY: all three arguments are NUL-terminated strings that outlive the call; rlottie
        // copies `data` and does not keep the pointers.
        let raw = locked(|| unsafe {
            lottie_animation_from_data(json.as_ptr(), c"".as_ptr(), c"".as_ptr())
        });
        let raw = NonNull::new(raw)
            .ok_or_else(|| Error::Runtime("rlottie cannot load this tgs".into()))?;
        // Owned from here on, so the early returns below destroy the handle.
        let mut anim = Animation { raw, frames: 0 };
        let (mut w, mut h) = (0usize, 0usize);
        // SAFETY: `raw` is a live handle from lottie_animation_from_data; `w` and `h` are valid
        // for writes.
        let frames = locked(|| unsafe {
            lottie_animation_get_size(raw.as_ptr(), &mut w, &mut h);
            lottie_animation_get_totalframe(raw.as_ptr())
        });
        if w == 0 || h == 0 {
            return Err(Error::Runtime("tgs has no canvas size".into()));
        }
        if frames == 0 {
            return Err(Error::Runtime("tgs has no frames".into()));
        }
        anim.frames = frames;
        Ok(anim)
    }

    pub fn frame_count(&self) -> usize {
        self.frames
    }

    /// Frame `index` (0-based from the animation start) rendered at `size`×`size`.
    pub fn render(&mut self, index: usize, size: u32) -> Result<Rgba, Error> {
        if index >= self.frames {
            return Err(Error::Runtime(format!(
                "frame {index} is out of range, the tgs has {} frames",
                self.frames
            )));
        }
        if size == 0 || size > MAX_SIZE {
            return Err(Error::Runtime(format!(
                "render size {size} is outside 1..={MAX_SIZE}"
            )));
        }
        let side = size as usize;
        let mut argb = vec![0u32; side * side];
        let raw = self.raw.as_ptr();
        // SAFETY: `raw` is live and `&mut self` makes this the only user of it; `argb` holds
        // `side` rows of `side` pixels, exactly `side * 4` bytes per row, as passed.
        locked(|| unsafe {
            lottie_animation_render(raw, index, argb.as_mut_ptr(), side, side, side * 4)
        });
        let mut img = Rgba::new(size, size);
        for (dst, &src) in img.px.as_chunks_mut::<4>().0.iter_mut().zip(&argb) {
            *dst = unpremultiply(src);
        }
        Ok(img)
    }
}

impl Drop for Animation {
    fn drop(&mut self) {
        let raw = self.raw.as_ptr();
        // SAFETY: `raw` came from lottie_animation_from_data and is destroyed only here, once.
        locked(|| unsafe { lottie_animation_destroy(raw) });
    }
}

/// Decompresses at most `MAX_JSON_BYTES`, so a gzip bomb never allocates its full size.
fn inflate(tgs: &[u8]) -> Result<Vec<u8>, Error> {
    let mut json = Vec::new();
    GzDecoder::new(tgs)
        .take(MAX_JSON_BYTES as u64 + 1)
        .read_to_end(&mut json)
        .map_err(|e| Error::Runtime(format!("tgs is not valid gzip: {e}")))?;
    if json.len() > MAX_JSON_BYTES {
        return Err(Error::Runtime(format!(
            "tgs JSON is larger than {} MiB",
            MAX_JSON_BYTES / (1024 * 1024)
        )));
    }
    Ok(json)
}

/// rlottie's premultiplied ARGB32 pixel as straight RGBA8.
fn unpremultiply(px: u32) -> [u8; 4] {
    let a = px >> 24;
    let un = |c: u32| {
        (c * 255 + a / 2)
            .checked_div(a)
            .map_or(0, |v| v.min(255) as u8)
    };
    [
        un((px >> 16) & 0xFF),
        un((px >> 8) & 0xFF),
        un(px & 0xFF),
        a as u8,
    ]
}
