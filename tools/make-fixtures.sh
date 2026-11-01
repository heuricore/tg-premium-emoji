#!/usr/bin/env bash
# Regenerates the raster test fixtures in crates/tge/tests/fixtures/:
#   alpha.webm     VP9 with alpha, 100x100, 30 fps, 2 s, encoded by libvpx-vp9
#   alpha.yuva420p the same video decoded by libvpx, raw yuva420p frames back to back
# Needs ffmpeg built with libvpx.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
out_dir="$repo_root/crates/tge/tests/fixtures"
webm="$out_dir/alpha.webm"
raw="$out_dir/alpha.yuva420p"

command -v ffmpeg >/dev/null || { echo "error: ffmpeg is not on PATH" >&2; exit 1; }
mkdir -p "$out_dir"
pass_dir="$(mktemp -d)"
trap 'rm -f -- "$pass_dir"/*; rmdir -- "$pass_dir"' EXIT

# A gradient disc whose radius and opacity pulse, with a one-pixel soft edge.
radius='(25+15*sin(PI*T))'
opacity='(175+80*cos(PI*T))'
disc="clip(${radius}-hypot(X-49.5\,Y-49.5)+0.5\,0\,1)"
graph="color=c=black:s=100x100:r=30:d=2,format=rgba"
graph+=",geq=r='255*X/W':g='255*Y/H':b='128+127*sin(2*PI*T)':a='${opacity}*${disc}'"

# Two passes, so libvpx adds hidden alt-ref frames packed into superframes, as real files have.
encode=(ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$graph"
  -c:v libvpx-vp9 -pix_fmt yuva420p -threads 1 -b:v 0 -crf 30 -passlogfile "$pass_dir/vp9")
"${encode[@]}" -pass 1 -f webm /dev/null
"${encode[@]}" -pass 2 -fflags +bitexact "$webm"

# libvpx-vp9 is forced as the decoder: it is the one that reads the alpha side stream.
ffmpeg -hide_banner -loglevel error -y -c:v libvpx-vp9 -i "$webm" \
  -f rawvideo -pix_fmt yuva420p "$raw"

frame_bytes=$((100 * 100 * 2 + 50 * 50 * 2))
expected=$((60 * frame_bytes))
actual=$(wc -c <"$raw")
if [[ "$actual" -ne "$expected" ]]; then
  echo "error: $raw has $actual bytes, expected $expected (60 frames)" >&2
  exit 1
fi
printf 'wrote %s and %s\n' "$webm" "$raw"
