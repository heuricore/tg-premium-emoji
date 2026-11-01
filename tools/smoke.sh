#!/usr/bin/env bash
# Renders the test fixtures with a tge binary into <out>/tgs/pulse and <out>/webm/alpha, for
# tools/ci_compare.py. The tgs is made from the hand-made Lottie with a deterministic gzip.
# Usage: tools/smoke.sh <runner prefix or ""> <binary> [out dir, default: out]
# Paths are relative to the repository root.
set -euo pipefail

runner=$1
bin=$2
out=${3:-out}
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
fixtures=crates/tge/tests/fixtures

mkdir -p "$out"
gzip -n -9 -c "$fixtures/pulse.json" >"$out/pulse.tgs"
# $runner is a command prefix such as "arch -x86_64"; word splitting is intended.
# shellcheck disable=SC2086
$runner "$bin" render "$out/pulse.tgs" --format tgs --out "$out/tgs/pulse"
# shellcheck disable=SC2086
$runner "$bin" render "$fixtures/alpha.webm" --format webm --out "$out/webm/alpha"
