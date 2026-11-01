//! Frame sampling, best-frame choice, the "frame suspect" metric and monochrome repaint.

use super::Rgba;

/// How many frames are sampled from an animation.
pub const SAMPLES: usize = 12;
/// A best frame covering less than this share of the canvas is suspect (provisional, tuned in Phase 2).
pub const SUSPECT_MIN_COVERAGE: f32 = 0.03;
/// A median frame covering less than this share of the best frame is suspect (provisional).
pub const SUSPECT_MIN_MEDIAN_RATIO: f32 = 0.35;

/// Coverage figures over the sampled frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameStats {
    /// Position of the best frame in the sampled list.
    pub best_index: usize,
    pub best: f32,
    pub median: f32,
    pub first: f32,
}

/// Up to `k` evenly spaced frame indices below `total`, including `0` and `total - 1`.
pub fn sample_positions(total: usize, k: usize) -> Vec<usize> {
    if total == 0 {
        return Vec::new();
    }
    if k <= 1 {
        return vec![0];
    }
    let mut out: Vec<usize> = Vec::with_capacity(k);
    for i in 0..k {
        let pos = (i as f64 * (total - 1) as f64 / (k - 1) as f64).round() as usize;
        if out.last() != Some(&pos) {
            out.push(pos);
        }
    }
    out
}

/// Mean alpha of the image, 0.0 to 1.0.
pub fn coverage(img: &Rgba) -> f32 {
    if img.px.is_empty() {
        return 0.0;
    }
    let sum: u64 = img
        .px
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| u64::from(p[3]))
        .sum();
    sum as f32 / (img.px.len() / 4) as f32 / 255.0
}

/// Picks the frame with the highest coverage (the earliest on a tie).
pub fn pick_best(coverages: &[f32]) -> FrameStats {
    let Some(&first) = coverages.first() else {
        return FrameStats {
            best_index: 0,
            best: 0.0,
            median: 0.0,
            first: 0.0,
        };
    };
    let mut best_index = 0;
    for (i, &c) in coverages.iter().enumerate() {
        if c > coverages[best_index] {
            best_index = i;
        }
    }
    let mut sorted = coverages.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mid = sorted.len() / 2;
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    };
    FrameStats {
        best_index,
        best: coverages[best_index],
        median,
        first,
    }
}

/// True when the best frame is nearly empty or the animation is unstable, so a single preview
/// frame may mislead.
pub fn is_frame_suspect(s: &FrameStats) -> bool {
    s.best < SUSPECT_MIN_COVERAGE || s.median / s.best < SUSPECT_MIN_MEDIAN_RATIO
}

/// Repaints every pixel white, keeping alpha.
pub fn whiten(img: &mut Rgba) {
    for p in img.px.as_chunks_mut::<4>().0 {
        p[..3].fill(255);
    }
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
    fn positions_cover_both_ends() {
        let p = sample_positions(180, 12);
        assert_eq!(p.len(), 12);
        assert_eq!(p[0], 0);
        assert_eq!(p[11], 179);
        assert!(p.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn positions_for_tiny_animation() {
        assert_eq!(sample_positions(1, 12), vec![0]);
        assert_eq!(sample_positions(5, 12), vec![0, 1, 2, 3, 4]);
        assert_eq!(sample_positions(0, 12), Vec::<usize>::new());
        assert_eq!(sample_positions(50, 1), vec![0]);
    }

    #[test]
    fn best_is_max_coverage_earliest_on_tie() {
        let s = pick_best(&[0.1, 0.4, 0.4, 0.2]);
        assert_eq!(s.best_index, 1);
        assert_eq!(s.best, 0.4);
        assert_eq!(s.first, 0.1);
        assert!((s.median - 0.3).abs() < 1e-6);
        assert_eq!(pick_best(&[0.1, 0.3, 0.2]).median, 0.2);
    }

    #[test]
    fn empty_best_is_suspect() {
        let s = pick_best(&[]);
        assert_eq!(
            s,
            FrameStats {
                best_index: 0,
                best: 0.0,
                median: 0.0,
                first: 0.0
            }
        );
        assert!(is_frame_suspect(&s));
    }

    #[test]
    fn unstable_animation_is_suspect() {
        assert!(is_frame_suspect(&pick_best(&[0.5, 0.05, 0.05, 0.05])));
    }

    #[test]
    fn steady_animation_is_not_suspect() {
        assert!(!is_frame_suspect(&pick_best(&[0.5, 0.45, 0.48, 0.5])));
    }

    #[test]
    fn coverage_is_mean_alpha() {
        assert_eq!(coverage(&solid(2, 2, [9, 9, 9, 255])), 1.0);
        assert_eq!(coverage(&Rgba::new(2, 2)), 0.0);
        assert_eq!(coverage(&Rgba::new(0, 0)), 0.0);
        let half = Rgba {
            w: 2,
            h: 1,
            px: vec![0, 0, 0, 255, 0, 0, 0, 0],
        };
        assert!((coverage(&half) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn whiten_keeps_alpha() {
        let mut img = Rgba {
            w: 2,
            h: 1,
            px: vec![10, 20, 30, 77, 1, 2, 3, 0],
        };
        whiten(&mut img);
        assert_eq!(img.px, vec![255, 255, 255, 77, 255, 255, 255, 0]);
    }
}
