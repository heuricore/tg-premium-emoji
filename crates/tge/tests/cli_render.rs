use std::io::Write;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use tge::render::frames::{SAMPLES, sample_positions};
use tge::render::webm;

const ALPHA_WEBM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/alpha.webm");
const PULSE: &[u8] = include_bytes!("fixtures/pulse.json");

fn tge() -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin("tge").unwrap()
}

/// Width, height and RGBA pixels of a PNG.
fn read_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

/// File names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    out.sort();
    out
}

fn png_names(indices: impl IntoIterator<Item = usize>) -> Vec<String> {
    let mut out: Vec<String> = indices.into_iter().map(|i| format!("{i}.png")).collect();
    out.sort();
    out
}

fn pulse_tgs(dir: &Path) -> std::path::PathBuf {
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    enc.write_all(PULSE).unwrap();
    let path = dir.join("pulse.tgs");
    std::fs::write(&path, enc.finish().unwrap()).unwrap();
    path
}

#[test]
fn webm_renders_every_frame_at_native_size() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("webm");
    let stdout = tge()
        .args(["--json", "render", ALPHA_WEBM, "--format", "webm", "--out"])
        .arg(&out)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(doc["frame_count"], 60);
    assert_eq!(doc["frames"].as_array().unwrap().len(), 60);
    assert_eq!(names(&out), png_names(0..60));

    let decoded = webm::decode_yuva(&std::fs::read(ALPHA_WEBM).unwrap()).unwrap();
    for index in [0, 59] {
        let expected = webm::to_rgba(&decoded[index]);
        let (w, h, px) = read_png(&out.join(format!("{index}.png")));
        assert_eq!((w, h), (100, 100));
        assert_eq!(px, expected.px, "frame {index}");
    }
}

#[test]
fn tgs_renders_sampled_frames_at_icon_size() {
    let dir = tempfile::tempdir().unwrap();
    let tgs = pulse_tgs(dir.path());
    let out = dir.path().join("tgs");
    tge()
        .arg("render")
        .arg(&tgs)
        .args(["--format", "tgs", "--out"])
        .arg(&out)
        .assert()
        .success();
    assert_eq!(names(&out), png_names(sample_positions(30, SAMPLES)));
    let (w, h, px) = read_png(&out.join("29.png"));
    assert_eq!((w, h), (128, 128));
    let opaque = px.as_chunks::<4>().0.iter().any(|p| p[3] > 0);
    assert!(opaque, "frame 29 is empty");
}

#[test]
fn size_and_frames_are_honoured() {
    let dir = tempfile::tempdir().unwrap();
    let tgs = pulse_tgs(dir.path());
    let out = dir.path().join("tgs");
    tge()
        .arg("render")
        .arg(&tgs)
        .args(["--format", "tgs", "--size", "64"])
        .args(["--frames", "0,29", "--out"])
        .arg(&out)
        .assert()
        .success();
    assert_eq!(names(&out), ["0.png", "29.png"]);
    assert_eq!(read_png(&out.join("0.png")).0, 64);

    let out = dir.path().join("webm");
    tge()
        .args(["render", ALPHA_WEBM, "--format", "webm", "--size", "50"])
        .args(["--frames", "3", "--out"])
        .arg(&out)
        .assert()
        .success();
    assert_eq!(names(&out), ["3.png"]);
    let (w, h, _) = read_png(&out.join("3.png"));
    assert_eq!((w, h), (50, 50));
}

#[test]
fn frame_out_of_range_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("webm");
    let stderr = tge()
        .args(["render", ALPHA_WEBM, "--format", "webm", "--frames", "0,60"])
        .arg("--out")
        .arg(&out)
        .assert()
        .code(1)
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains("frame 60 is out of range"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn render_is_hidden_from_help() {
    let out = tge().arg("--help").assert().success();
    let help = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(!help.contains("render"), "{help}");
}
