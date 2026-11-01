"""Compare every platform's smoke output (tools/smoke.sh) against linux-x64.

Usage: ci_compare.py <dir holding out-<platform>/ folders>
webm frames must have identical pixels; tgs frames may differ in at most BAD_LIMIT % of pixels
(a pixel is bad when a premultiplied channel differs by more than 48). Exit 1 on a mismatch, a
missing frame, or a reference with no frames.
"""

import os
import sys
from pathlib import Path

import numpy as np
from PIL import Image

BAD_LIMIT = 2.0
REFERENCE = "out-linux-x64"


def rgba(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGBA"), dtype=np.float32)


def premul(path: Path) -> np.ndarray:
    a = rgba(path)
    a[..., :3] *= a[..., 3:4] / 255.0
    return a


def webm_identical(ref: Path, plat: Path) -> bool:
    for r in ref.glob("webm/*/*.png"):
        o = plat / r.relative_to(ref)
        if not o.is_file():
            return False
        a, b = Image.open(r).convert("RGBA"), Image.open(o).convert("RGBA")
        if a.size != b.size or a.tobytes() != b.tobytes():
            return False
    return True


def tgs_worst(ref: Path, plat: Path) -> "tuple[float, float]":
    worst_bad, worst_mae = 0.0, 0.0
    for r in ref.glob("tgs/*/*.png"):
        o = plat / r.relative_to(ref)
        if not o.is_file():
            return 100.0, 255.0
        a, b = premul(r), premul(o)
        if a.shape != b.shape:
            return 100.0, 255.0
        d = np.abs(a - b)
        worst_bad = max(worst_bad, float((d.max(axis=2) > 48).mean() * 100))
        worst_mae = max(worst_mae, float(d.mean()))
    return worst_bad, worst_mae


def main() -> int:
    root = Path(sys.argv[1])
    ref = root / REFERENCE
    counts = {kind: len(list(ref.glob(kind + "/*/*.png"))) for kind in ("webm", "tgs")}
    lines = [
        "reference %s: %d webm and %d tgs frames" % (REFERENCE, counts["webm"], counts["tgs"]),
        "",
        "| platform | webm pixels | tgs worst bad% | tgs worst MAE |",
        "|---|---|---|---|",
    ]
    failed = not all(counts.values())
    for plat in sorted(p for p in root.iterdir() if p.is_dir()):
        webm_ok = webm_identical(ref, plat)
        worst_bad, worst_mae = tgs_worst(ref, plat)
        ok = webm_ok and worst_bad <= BAD_LIMIT
        failed |= not ok
        lines.append(
            "| %s | %s | %.2f%s | %.2f |"
            % (
                plat.name[len("out-") :] if plat.name.startswith("out-") else plat.name,
                "identical" if webm_ok else "DIFFERS",
                worst_bad,
                "" if worst_bad <= BAD_LIMIT else " FAIL",
                worst_mae,
            )
        )
    report = "\n".join(lines)
    print(report)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write(report + "\n")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
