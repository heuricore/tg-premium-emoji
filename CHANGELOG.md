# Changelog

## 0.0.1 (2026-11-01)

First pre-release (Phase 1).

- `tge` CLI: `pack build` from the Bot API with numbered preview sheets (tgs through vendored
  rlottie, webm VP9 with alpha and webp in pure Rust), rescans that keep labels by custom emoji id,
  `pack preview`, `label`, `similar`, `report`, `find` (BM25 over labels, Russian and English),
  `snippet` (HTML, MarkdownV2, button field), `validate`, `catalogs`, `doctor`.
- Packs up to 1000 emoji by default; `pack build --no-limit` lifts the limit.
- Built-in catalog with the labeled `tgiosicons` pack (399 icons).
- Skills: `guide`, `pack-build`, `emoji-design`.
- `scripts/install.py`: versioned installs of `tge` from GitHub Releases, `--check`, `--update`.
- Release builds for Linux x64/arm64, macOS arm64/x64 and Windows x64, depending only on the
  system libc.
