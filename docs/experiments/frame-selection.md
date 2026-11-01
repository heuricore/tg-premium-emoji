# Frame selection: notes from `tgiosicons`

Phase 1 picks the preview frame of an animated emoji by sampling 12 evenly spaced frames,
measuring alpha coverage (mean alpha / 255) on each, and keeping the frame with the highest
coverage (earliest on a tie). `frame_suspect` is set when the best coverage is below `0.03` or
when `median / best` is below `0.35`. Both thresholds were provisional; this note records what the
first real pack showed. Data: `tge --catalog-dir catalog pack build tgiosicons --json` (the
`frames` array), built 2026-11-01.

## The pack

`tgiosicons` has 399 emoji. All are `tgs` and all have `needs_repainting: true` (monochrome
interface icons, whitened on the sheets). There were no decode errors.

## Results

| Measure | Value |
|---|---|
| `best_index` ≠ 0 (best frame is not the first sampled one) | 0 of 399 |
| Coverage gap, best minus first frame | 0.0000 for every emoji |
| Best coverage: min / median / max | 0.054 / 0.134 / 0.448 |
| `median / best`: min / mean | 0.694 / 0.999 |
| `frame_suspect` with the provisional thresholds | 0 |

Almost every icon is a still image wrapped in Lottie: all 12 sampled frames are the same, so the
choice of frame does not matter for this pack. Only one emoji visibly changes over time.

## Examples

- **n 364, "three sparkles"** (sheet 19): the only animated icon. Coverage is 0.135 on the first
  frame and the median is 0.094 (ratio 0.69), so the sparkles shrink or fade during the loop. The
  first frame is already the fullest, and the sheet shows all three sparkles. The choice was
  right, and the frame was not suspect: 0.69 is well above 0.35.
- **n 31, "pencil"** (sheet 2): the lowest coverage in the pack, 0.054. This is a thin
  line icon, not an empty frame. The `0.03` floor sits below the thinnest real icon here with
  some margin. Raising it towards 0.05 would start flagging legitimate line icons.
- **n 27, "left arrow"** (sheet 2), and **n 270, "small hashtag"** (sheet 14): 0.068 and 0.069,
  the same pattern as the pencil. Thin strokes give low coverage, which a coverage-only metric
  cannot tell apart from "mostly empty".

## What this does not tell us

This pack does not exercise the metric. It has no colourful animation, no emoji that starts empty
and fades in, and no loops where the first frame is misleading. The thresholds stay provisional.
Tuning them needs a second, colourful animated pack, which is Phase 2 work (spec 5.8).
