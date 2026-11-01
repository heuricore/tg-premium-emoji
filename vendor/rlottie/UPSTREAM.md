# Vendored rlottie

- Repository: https://github.com/desktop-app/rlottie (the fork used by Telegram Desktop)
- Commit: `cbfdcdd2830b4beba53e64e72c11cb0328a720a9`
- License: LGPL-2.1, with the bundled parts listed in `COPYING` (texts in `licenses/`)

## Kept

`src/`, `inc/`, `licenses/`, `COPYING`, `AUTHORS`, as they are upstream. The `CMakeLists.txt`
and `meson.build` files inside `src/` and `inc/` are kept but unused.

## Dropped

- Folders: `example/`, `test/`, `.Gifs/`, `vs2019/`, `cmake/`, `packaging/`
- Files: `src/binding/.lottieplayer.cpp.swp` (stray editor swap file), `.appveyor.yml`,
  `.travis.yml`, `.clang-format`, `.gitignore`, `CMakeLists.txt`, `meson.build`,
  `meson_options.txt`, `rlottie.pc.in`, `README.md`

## Local patches

None. The library is compiled by `crates/tge/build.rs` with the `cc` crate.
