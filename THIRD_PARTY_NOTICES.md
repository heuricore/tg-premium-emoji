# Third-party notices

`tge` itself is MIT-licensed (see `LICENSE`). It includes the following third-party code.

## rlottie

- Source: `vendor/rlottie/`, from https://github.com/desktop-app/rlottie at commit
  `cbfdcdd2830b4beba53e64e72c11cb0328a720a9` (details in `vendor/rlottie/UPSTREAM.md`)
- License: GNU Lesser General Public License 2.1, `vendor/rlottie/licenses/COPYING.LGPL`
- Used to render tgs (Lottie) emoji. It is compiled from the sources in this repository and
  linked statically into `tge`, so `tge` can be rebuilt with a modified rlottie from this
  repository.

rlottie bundles code under other licenses (see `vendor/rlottie/COPYING`):

| Part | Folder | License | Text |
|---|---|---|---|
| FreeType rasterizer and stroker | `src/vector/freetype` | FreeType License (FTL) | `vendor/rlottie/licenses/COPYING.FTL` |
| pixman region code | `src/vector/pixman` | MIT | `vendor/rlottie/licenses/COPYING.PIX` |
| stb_image | `src/vector/stb` | MIT | `vendor/rlottie/licenses/COPYING.STB` |
| RapidJSON | `src/lottie/rapidjson` | MIT | `vendor/rlottie/licenses/COPYING.RPD` |

## vp9dec

- Source: the `vp9dec` crate from crates.io, https://github.com/ChanyaVRC/vp9dec
- License: MIT
- Used to decode webm (VP9) emoji.

```
MIT License

Copyright (c) 2026 vp9dec contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Other Rust dependencies come from crates.io under their own licenses; `Cargo.lock` lists them.
