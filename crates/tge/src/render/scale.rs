//! Bilinear scaling of straight RGBA images.

use super::Rgba;

/// Resizes `src` to `w`×`h` with bilinear interpolation on premultiplied values.
pub fn resize(src: &Rgba, w: u32, h: u32) -> Rgba {
    let mut out = Rgba::new(w, h);
    if src.w == 0 || src.h == 0 || w == 0 || h == 0 {
        return out;
    }
    let premul: Vec<[f32; 4]> = src
        .px
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let a = f32::from(p[3]);
            let k = a / 255.0;
            [
                f32::from(p[0]) * k,
                f32::from(p[1]) * k,
                f32::from(p[2]) * k,
                a,
            ]
        })
        .collect();
    let xs = taps(src.w, w);
    let ys = taps(src.h, h);
    let sw = src.w as usize;
    for (y, &(y0, y1, fy)) in ys.iter().enumerate() {
        for (x, &(x0, x1, fx)) in xs.iter().enumerate() {
            let mut acc = [0.0f32; 4];
            for (c, slot) in acc.iter_mut().enumerate() {
                let top = premul[y0 * sw + x0][c] * (1.0 - fx) + premul[y0 * sw + x1][c] * fx;
                let bottom = premul[y1 * sw + x0][c] * (1.0 - fx) + premul[y1 * sw + x1][c] * fx;
                *slot = top * (1.0 - fy) + bottom * fy;
            }
            let o = (y * w as usize + x) * 4;
            let a = acc[3];
            if a.round() >= 1.0 {
                for (dst, &v) in out.px[o..o + 3].iter_mut().zip(&acc) {
                    *dst = (v * 255.0 / a).round().clamp(0.0, 255.0) as u8;
                }
                out.px[o + 3] = a.round().min(255.0) as u8;
            }
        }
    }
    out
}

/// Fits `src` inside `size`×`size` keeping its aspect ratio, centred on a transparent canvas.
pub fn fit_square(src: &Rgba, size: u32) -> Rgba {
    if src.w == src.h {
        return resize(src, size, size);
    }
    let long = src.w.max(src.h);
    let scaled = |side: u32| {
        ((u64::from(side) * u64::from(size) + u64::from(long) / 2) / u64::from(long)).max(1) as u32
    };
    let (w, h) = (scaled(src.w), scaled(src.h));
    let inner = resize(src, w, h);
    let mut canvas = Rgba::new(size, size);
    let (ox, oy) = ((size - w) / 2, (size - h) / 2);
    for row in 0..h as usize {
        let from = row * w as usize * 4;
        let to = ((oy as usize + row) * size as usize + ox as usize) * 4;
        canvas.px[to..to + w as usize * 4].copy_from_slice(&inner.px[from..from + w as usize * 4]);
    }
    canvas
}

/// For each destination index: the two neighbouring source indices and the weight of the second,
/// with pixel-centre alignment and edge clamping.
fn taps(src_len: u32, dst_len: u32) -> Vec<(usize, usize, f32)> {
    let ratio = f64::from(src_len) / f64::from(dst_len);
    let last = f64::from(src_len - 1);
    (0..dst_len)
        .map(|i| {
            let s = ((f64::from(i) + 0.5) * ratio - 0.5).clamp(0.0, last);
            let lo = s.floor();
            let hi = (lo + 1.0).min(last);
            (lo as usize, hi as usize, (s - lo) as f32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Rgba {
        Rgba {
            w,
            h,
            px: px.repeat(w as usize * h as usize),
        }
    }

    #[test]
    fn resize_keeps_opaque_opaque() {
        let out = resize(&solid(100, 100, [10, 200, 30, 255]), 128, 128);
        assert_eq!((out.w, out.h), (128, 128));
        assert!(out.px.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn resize_keeps_single_colour_exactly() {
        let out = resize(&solid(7, 5, [13, 99, 250, 255]), 31, 17);
        assert!(
            out.px
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [13, 99, 250, 255])
        );
        let down = resize(&solid(100, 100, [13, 99, 250, 255]), 3, 3);
        assert!(
            down.px
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [13, 99, 250, 255])
        );
    }

    #[test]
    fn resize_same_size_is_identity() {
        let src = Rgba {
            w: 2,
            h: 2,
            px: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 0, 0, 0, 0],
        };
        assert_eq!(resize(&src, 2, 2), src);
    }

    #[test]
    fn resize_transparent_pixels_have_zero_rgb() {
        let out = resize(&Rgba::new(4, 4), 9, 9);
        assert!(out.px.iter().all(|&b| b == 0));
    }

    #[test]
    fn resize_does_not_bleed_colour_from_transparent_neighbours() {
        // Left pixel opaque red, right pixel fully transparent with garbage RGB.
        let src = Rgba {
            w: 2,
            h: 1,
            px: vec![255, 0, 0, 255, 0, 255, 0, 0],
        };
        let out = resize(&src, 4, 1);
        for p in out.px.as_chunks::<4>().0 {
            if p[3] > 0 {
                assert_eq!(&p[..3], &[255, 0, 0]);
            }
        }
        assert!(out.px[7] < 255);
    }

    #[test]
    fn resize_midpoint_averages() {
        let src = Rgba {
            w: 2,
            h: 1,
            px: vec![0, 0, 0, 255, 200, 200, 200, 255],
        };
        let out = resize(&src, 4, 1);
        assert_eq!(out.px[0], 0);
        assert_eq!(out.px[4], 50);
        assert_eq!(out.px[8], 150);
        assert_eq!(out.px[12], 200);
    }
}
