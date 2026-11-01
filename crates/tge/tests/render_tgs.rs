use std::io::Write;
use std::thread;

use flate2::Compression;
use flate2::write::GzEncoder;
use tge::model::pack::Format;
use tge::render::tgs::MAX_JSON_BYTES;
use tge::render::{Media, Rgba};

const PULSE: &[u8] = include_bytes!("fixtures/pulse.json");

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn pulse() -> Media {
    Media::open(Format::Tgs, &gzip(PULSE)).unwrap()
}

fn coverage(img: &Rgba) -> f64 {
    let sum: u64 = img
        .px
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| u64::from(p[3]))
        .sum();
    sum as f64 / (255.0 * f64::from(img.w * img.h))
}

#[test]
fn pulse_has_30_frames() {
    assert_eq!(pulse().frame_count(), 30);
}

#[test]
fn frame_has_requested_size() {
    let img = pulse().frame(0, 128).unwrap();
    assert_eq!((img.w, img.h), (128, 128));
    assert_eq!(img.px.len(), 128 * 128 * 4);
}

#[test]
fn animation_grows() {
    let mut media = pulse();
    let first = coverage(&media.frame(0, 128).unwrap());
    let last = coverage(&media.frame(29, 128).unwrap());
    assert!(first > 0.0, "frame 0 is empty");
    assert!(
        last > first,
        "frame 29 coverage {last} <= frame 0 coverage {first}"
    );
}

#[test]
fn pixels_are_straight_rgba() {
    let img = pulse().frame(29, 128).unwrap();
    let centre = (64 * 128 + 64) * 4;
    assert_eq!(&img.px[centre..centre + 4], &[255, 0, 0, 255]);
    let edge: Vec<&[u8; 4]> = img
        .px
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0 && p[3] < 255)
        .collect();
    assert!(!edge.is_empty(), "no anti-aliased edge pixels");
    for p in edge {
        assert!(p[0] >= 250 && p[1] == 0 && p[2] == 0, "{p:?} is not red");
    }
}

#[test]
fn bad_frame_requests_are_errors() {
    let mut media = pulse();
    assert!(media.frame(29, 1).is_ok());
    assert!(media.frame(30, 128).is_err());
    assert!(media.frame(0, 0).is_err());
}

#[test]
fn parallel_renders_match_serial() {
    let render_all = || {
        let mut media = pulse();
        (0..30)
            .map(|i| media.frame(i, 128).unwrap())
            .collect::<Vec<_>>()
    };
    let serial = render_all();
    let threads: Vec<_> = (0..8).map(|_| thread::spawn(render_all)).collect();
    for t in threads {
        assert!(t.join().unwrap() == serial, "parallel render differs");
    }
}

#[test]
fn garbage_is_error_not_panic() {
    let mut x: u32 = 0x9E37_79B9;
    let bytes: Vec<u8> = (0..4096)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect();
    assert!(Media::open(Format::Tgs, &bytes).is_err());
}

#[test]
fn gzipped_non_lottie_is_error() {
    let cases: [&[u8]; 6] = [
        b"",
        b"not json",
        b"{}",
        b"{\"v\": \"5\0\"}",
        b"{\"v\": \"5.7\", \"ip\": 0, \"op\": 30, \"layers\": []}",
        b"{\"v\": \"5.7\", \"w\": 100, \"h\": 100, \"ip\": 0, \"op\": 0, \"layers\": []}",
    ];
    for json in cases {
        let res = Media::open(Format::Tgs, &gzip(json));
        assert!(res.is_err(), "{:?} opened", String::from_utf8_lossy(json));
    }
}

#[test]
fn oversized_json_is_rejected() {
    let spaces = vec![b' '; 17 * 1024 * 1024];
    let err = Media::open(Format::Tgs, &gzip(&spaces)).unwrap_err();
    assert!(err.to_string().contains("16 MiB"), "{err}");
}

#[test]
fn json_at_the_cap_is_accepted() {
    let mut json = PULSE.to_vec();
    json.resize(MAX_JSON_BYTES, b' ');
    assert_eq!(
        Media::open(Format::Tgs, &gzip(&json))
            .unwrap()
            .frame_count(),
        30
    );
}
