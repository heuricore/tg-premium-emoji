use image_webp::{ColorType, WebPEncoder};
use tge::model::pack::Format;
use tge::render::webm::{self, YuvaFrame};
use tge::render::{Media, Rgba, webp};

/// libvpx-vp9 encode of a pulsing gradient disc with alpha; see `tools/make-fixtures.sh`.
const ALPHA_WEBM: &[u8] = include_bytes!("fixtures/alpha.webm");
/// `ALPHA_WEBM` decoded by libvpx: 60 yuva420p frames back to back.
const ALPHA_YUVA: &[u8] = include_bytes!("fixtures/alpha.yuva420p");
const FRAME_BYTES: usize = 100 * 100 * 2 + 50 * 50 * 2;
/// libwebp lossy 32x32 with alpha (VP8X, ALPH and `VP8 ` chunks): red grows downwards, alpha
/// rightwards.
const LOSSY_ALPHA_WEBP: &[u8] = include_bytes!("fixtures/lossy-alpha.webp");

const SEGMENT: [u8; 4] = [0x18, 0x53, 0x80, 0x67];
const CLUSTER: [u8; 4] = [0x1F, 0x43, 0xB6, 0x75];
/// Start of a shown profile 0 VP9 key frame: frame marker byte, then the sync code.
const KEY_FRAME: [u8; 4] = [0x82, 0x49, 0x83, 0x42];

fn pseudo_random(seed: u32, len: usize) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

fn find(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("pattern is in the fixture")
}

/// One EBML element with an 8-byte size field.
fn ebml(id: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.push(0x01);
    out.extend_from_slice(&(body.len() as u64).to_be_bytes()[1..]);
    out.extend_from_slice(body);
    out
}

/// A webm with one VP9 track and one SimpleBlock carrying `flags` and `payload`.
fn webm_with_simple_block(flags: u8, payload: &[u8]) -> Vec<u8> {
    let video = ebml(
        &[0xE0],
        &[ebml(&[0xB0], &[100]), ebml(&[0xBA], &[100])].concat(),
    );
    let entry = [ebml(&[0xD7], &[1]), ebml(&[0x86], b"V_VP9"), video].concat();
    let tracks = ebml(&[0x16, 0x54, 0xAE, 0x6B], &ebml(&[0xAE], &entry));
    let mut block = vec![0x81, 0x00, 0x00, flags];
    block.extend_from_slice(payload);
    let cluster = ebml(
        &CLUSTER,
        &[ebml(&[0xE7], &[0]), ebml(&[0xA3], &block)].concat(),
    );
    let header = ebml(&[0x1A, 0x45, 0xDF, 0xA3], &ebml(&[0x42, 0x82], b"webm"));
    [header, ebml(&SEGMENT, &[tracks, cluster].concat())].concat()
}

/// The fixture with the size of the first element `id` replaced by EBML's "unknown size".
fn with_unknown_size(id: &[u8]) -> Vec<u8> {
    let mut bytes = ALPHA_WEBM.to_vec();
    let size_at = find(&bytes, id) + id.len();
    let len = bytes[size_at].leading_zeros() as usize + 1;
    bytes[size_at] = 0xFF >> (len - 1);
    bytes[size_at + 1..size_at + len].fill(0xFF);
    bytes
}

fn concat_planes(frames: &[YuvaFrame]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(frames.len() * FRAME_BYTES);
    for f in frames {
        for plane in [&f.y, &f.u, &f.v, &f.a] {
            raw.extend_from_slice(plane);
        }
    }
    raw
}

fn pixel(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
    let at = (y * img.w + x) as usize * 4;
    img.px[at..at + 4].try_into().unwrap()
}

fn encode_webp(data: &[u8], w: u32, h: u32, color: ColorType) -> Vec<u8> {
    let mut out = Vec::new();
    WebPEncoder::new(&mut out)
        .encode(data, w, h, color)
        .unwrap();
    out
}

fn riff_chunk(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = fourcc.to_vec();
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn riff_webp(chunks: &[Vec<u8>]) -> Vec<u8> {
    let body = chunks.concat();
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&body);
    out
}

/// VP8X chunk: feature flags, then the canvas size.
fn vp8x(flags: u8, w: u32, h: u32) -> Vec<u8> {
    let mut payload = vec![flags, 0, 0, 0];
    payload.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
    payload.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
    riff_chunk(b"VP8X", &payload)
}

/// `VP8 ` chunk whose key frame header declares `w`x`h`, followed by noise.
fn vp8_chunk(w: u16, h: u16) -> Vec<u8> {
    let mut payload = vec![0x10, 0x02, 0x00, 0x9D, 0x01, 0x2A];
    payload.extend_from_slice(&w.to_le_bytes());
    payload.extend_from_slice(&h.to_le_bytes());
    payload.extend(pseudo_random(7, 256));
    riff_chunk(b"VP8 ", &payload)
}

/// VP8L chunk whose header declares `w`x`h`, followed by noise.
fn vp8l_chunk(w: u32, h: u32) -> Vec<u8> {
    let mut payload = vec![0x2F];
    payload.extend_from_slice(&((w - 1) | ((h - 1) << 14)).to_le_bytes());
    payload.extend(pseudo_random(7, 256));
    riff_chunk(b"VP8L", &payload)
}

#[test]
fn webm_frame_count_and_size() {
    let mut media = Media::open(Format::Webm, ALPHA_WEBM).unwrap();
    assert_eq!(media.frame_count(), 60);
    for index in [0, 59] {
        let img = media.frame(index, 128).unwrap();
        assert_eq!((img.w, img.h), (128, 128));
        assert_eq!(img.px.len(), 128 * 128 * 4);
    }
    assert!(media.frame(60, 128).is_err());
}

#[test]
fn webm_matches_libvpx_bytes() {
    let raw = concat_planes(&webm::decode_yuva(ALPHA_WEBM).unwrap());
    assert_eq!(raw.len(), ALPHA_YUVA.len(), "decoded size");
    if let Some(at) = raw.iter().zip(ALPHA_YUVA).position(|(a, b)| a != b) {
        panic!("first difference at byte {at} (frame {})", at / FRAME_BYTES);
    }
}

#[test]
fn webm_frame_is_straight_rgba() {
    let img = Media::open(Format::Webm, ALPHA_WEBM)
        .unwrap()
        .frame(0, 100)
        .unwrap();
    assert_eq!(pixel(&img, 0, 0)[3], 0, "corner is transparent");
    // Inside the disc of frame 0 the source is (255 * x / 100, 255 * y / 100, 128), fully opaque.
    let [r, g, b, a] = pixel(&img, 70, 50);
    let near = |got: u8, want: u8| got.abs_diff(want) <= 12;
    assert!(
        near(r, 178) && near(g, 127) && near(b, 128) && a >= 250,
        "{:?}",
        [r, g, b, a]
    );
}

#[test]
fn to_rgba_is_bt601_limited_range() {
    let frame = YuvaFrame {
        w: 2,
        h: 2,
        y: vec![16, 235, 81, 126],
        u: vec![128],
        v: vec![128],
        a: vec![0, 85, 170, 255],
    };
    let img = webm::to_rgba(&frame);
    assert_eq!((img.w, img.h), (2, 2));
    assert_eq!(
        img.px,
        [
            0, 0, 0, 0, 255, 255, 255, 85, 76, 76, 76, 170, 128, 128, 128, 255
        ]
    );
}

#[test]
fn to_rgba_handles_odd_sizes() {
    // 3x3 luma with 2x2 chroma: the last row and column share the chroma samples of index 1.
    let frame = YuvaFrame {
        w: 3,
        h: 3,
        y: vec![81; 9],
        u: vec![128, 128, 128, 90],
        v: vec![128, 128, 128, 240],
        a: (0..9).map(|i| i * 10).collect(),
    };
    let img = webm::to_rgba(&frame);
    assert_eq!(img.px.len(), 3 * 3 * 4);
    assert_eq!(pixel(&img, 2, 2), [255, 0, 0, 80], "pure red");
    for (x, y) in [(0, 0), (1, 1), (2, 1), (1, 2)] {
        let [r, g, b, a] = pixel(&img, x, y);
        assert_eq!([r, g, b], [76, 76, 76], "grey at ({x}, {y})");
        assert_eq!(a, (y * 3 + x) as u8 * 10);
    }
}

#[test]
fn laced_block_is_error() {
    for lacing in [0x02, 0x04, 0x06] {
        let bytes = webm_with_simple_block(0x80 | lacing, &[0x00; 16]);
        let err = webm::decode_yuva(&bytes).unwrap_err();
        assert!(err.to_string().contains("lacing"), "{lacing:#x}: {err}");
    }
}

#[test]
fn garbage_webm_is_error_not_panic() {
    for seed in [0x9E37_79B9, 0x1234_5678, 0xDEAD_BEEF] {
        let bytes = pseudo_random(seed, 4096);
        assert!(Media::open(Format::Webm, &bytes).is_err(), "seed {seed:#x}");
    }
}

#[test]
fn truncated_webm_is_error() {
    let len = ALPHA_WEBM.len();
    for cut in [0, 1, 4, 40, 300, 420, 440, len / 4, len / 2, len - 1] {
        assert!(
            webm::decode_yuva(&ALPHA_WEBM[..cut]).is_err(),
            "cut at {cut} of {len}"
        );
    }
}

#[test]
fn corrupt_vp9_data_never_panics() {
    let key = find(ALPHA_WEBM, &KEY_FRAME);
    for seed in 1..=24u32 {
        let mut bytes = ALPHA_WEBM.to_vec();
        let noise = pseudo_random(seed.wrapping_mul(0x9E37_79B9), 16);
        for &[offset, flip] in noise.as_chunks::<2>().0 {
            bytes[key + 8 + usize::from(offset) * 2] ^= flip | 1;
        }
        // Ok with damaged pixels and Err are both fine; a panic fails the test.
        let _ = webm::decode_yuva(&bytes);
    }
}

#[test]
fn oversized_vp9_frame_is_rejected() {
    let mut bytes = ALPHA_WEBM.to_vec();
    // frame_width_minus_1 sits 4 bits into the byte after the sync code: 1999 -> 2000 wide.
    let size = find(&bytes, &KEY_FRAME) + KEY_FRAME.len();
    bytes[size] &= 0xF0;
    bytes[size + 1] = 0x7C;
    bytes[size + 2] = (bytes[size + 2] & 0x0F) | 0xF0;
    let err = webm::decode_yuva(&bytes).unwrap_err();
    assert!(err.to_string().contains("1024"), "{err}");
}

#[test]
fn unknown_size_segment_is_read_to_the_end() {
    let frames = webm::decode_yuva(&with_unknown_size(&SEGMENT)).unwrap();
    assert!(concat_planes(&frames) == ALPHA_YUVA, "frames differ");
}

#[test]
fn unknown_size_cluster_is_error() {
    let err = webm::decode_yuva(&with_unknown_size(&CLUSTER)).unwrap_err();
    assert!(err.to_string().contains("unknown size"), "{err}");
}

/// 100x100 RGBA with every channel varying, alpha included.
fn gradient() -> Vec<u8> {
    (0..100u32)
        .flat_map(|y| (0..100u32).map(move |x| (x, y)))
        .flat_map(|(x, y)| [x * 2, y * 2, 255 - x - y, 255 - x].map(|c| c as u8))
        .collect()
}

#[test]
fn webp_roundtrip() {
    let rgba = gradient();
    let bytes = encode_webp(&rgba, 100, 100, ColorType::Rgba8);
    let mut media = Media::open(Format::Static, &bytes).unwrap();
    assert_eq!(media.frame_count(), 1);
    let img = media.frame(0, 100).unwrap();
    assert_eq!((img.w, img.h), (100, 100));
    assert!(img.px == rgba, "pixels differ");
    assert!(media.frame(1, 100).is_err());
    assert_eq!(media.frame(0, 128).unwrap().w, 128);
}

#[test]
fn opaque_webp_gets_alpha_255() {
    let rgb: Vec<u8> = (0..16 * 8 * 3).map(|i| (i * 7) as u8).collect();
    let img = webp::decode(&encode_webp(&rgb, 16, 8, ColorType::Rgb8)).unwrap();
    let expected: Vec<u8> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|&[r, g, b]| [r, g, b, 255])
        .collect();
    assert_eq!((img.w, img.h), (16, 8));
    assert_eq!(img.px, expected);
}

#[test]
fn oversized_webp_is_rejected() {
    let bytes = encode_webp(&[0; 1100 * 4], 1100, 1, ColorType::Rgba8);
    let err = webp::decode(&bytes).unwrap_err();
    assert!(err.to_string().contains("1024"), "{err}");
}

#[test]
fn garbage_and_truncated_webp_are_errors() {
    assert!(webp::decode(&pseudo_random(0x9E37_79B9, 4096)).is_err());
    let bytes = encode_webp(&gradient(), 100, 100, ColorType::Rgba8);
    for cut in [0, 12, 20, 30, bytes.len() / 2] {
        assert!(webp::decode(&bytes[..cut]).is_err(), "cut at {cut}");
    }
}

#[test]
fn lossy_webp_with_alpha_decodes() {
    let img = webp::decode(LOSSY_ALPHA_WEBP).unwrap();
    assert_eq!((img.w, img.h), (32, 32));
    assert!(pixel(&img, 0, 16)[3] < 16, "left edge is transparent");
    let [top_r, _, _, top_a] = pixel(&img, 31, 0);
    let [bottom_r, _, _, bottom_a] = pixel(&img, 31, 31);
    assert!(top_a > 230 && bottom_a > 230, "right edge is opaque");
    assert!(
        top_r < 40 && bottom_r > 200,
        "red {top_r} at top, {bottom_r} at bottom"
    );
}

#[test]
fn damaged_webp_never_panics() {
    // Every prefix, then byte flips anywhere, chunk sizes and frame headers included.
    for cut in 0..LOSSY_ALPHA_WEBP.len() {
        let _ = webp::decode(&LOSSY_ALPHA_WEBP[..cut]);
    }
    for seed in 1..=300u32 {
        let mut bytes = LOSSY_ALPHA_WEBP.to_vec();
        let noise = pseudo_random(seed.wrapping_mul(0x9E37_79B9), 6);
        for &[at, flip] in noise.as_chunks::<2>().0 {
            let at = usize::from(at) % bytes.len();
            bytes[at] ^= flip | 1;
        }
        let _ = webp::decode(&bytes);
    }
}

#[test]
fn extended_lossless_webp_decodes() {
    let rgba = gradient();
    let mut bytes = Vec::new();
    let mut encoder = WebPEncoder::new(&mut bytes);
    encoder.set_exif_metadata(b"Exif\0\0".to_vec());
    encoder.encode(&rgba, 100, 100, ColorType::Rgba8).unwrap();
    assert_eq!(&bytes[12..16], b"VP8X");
    assert!(webp::decode(&bytes).unwrap().px == rgba, "pixels differ");
}

#[test]
fn webp_frame_must_match_canvas() {
    // The image chunk is checked before decoding, so the noise after its header is never read.
    for image in [vp8_chunk(2000, 2000), vp8l_chunk(2000, 2000)] {
        let bytes = riff_webp(&[vp8x(0, 100, 100), image]);
        let err = webp::decode(&bytes).unwrap_err();
        assert!(err.to_string().contains("2000x2000"), "{err}");
    }
}

#[test]
fn animated_webp_is_rejected() {
    // ANMF payload: x, y, width - 1, height - 1, duration (3 bytes each), flags, then the frame.
    let mut frame = vec![0; 6];
    frame.extend_from_slice(&[99, 0, 0, 99, 0, 0, 100, 0, 0, 0]);
    frame.extend(vp8_chunk(100, 100));
    let flagged = riff_webp(&[
        vp8x(0x02, 100, 100),
        riff_chunk(b"ANIM", &[0; 6]),
        riff_chunk(b"ANMF", &frame),
    ]);
    // A still VP8X image that also carries an animation frame without the animation flag.
    let still = encode_webp(&[9; 2 * 2 * 4], 2, 2, ColorType::Rgba8);
    let stray = riff_webp(&[
        vp8x(0x10, 2, 2),
        still[12..].to_vec(),
        riff_chunk(b"ANMF", &[0; 24]),
    ]);
    for bytes in [flagged, stray] {
        let err = webp::decode(&bytes).unwrap_err();
        assert!(err.to_string().contains("animated"), "{err}");
    }
}
