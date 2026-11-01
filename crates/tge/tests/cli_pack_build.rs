use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use flate2::Compression;
use flate2::write::GzEncoder;
use image_webp::{ColorType, WebPEncoder};
use serde_json::{Value, json};
use tge::validate::{Severity, validate_catalog_dir};

/// Obviously fake; only ever handed to the child process and the local mock.
const TOKEN: &str = "123456:TEST-fake-token";
const PULSE: &[u8] = include_bytes!("fixtures/pulse.json");
const ALPHA_WEBM: &[u8] = include_bytes!("fixtures/alpha.webm");

const ID_TGS: &str = "5000000000000000001";
const ID_WEBM: &str = "5000000000000000002";
const ID_WEBP: &str = "5000000000000000003";
const BG: [u8; 4] = [0x1E, 0x21, 0x27, 0xFF];

#[derive(Clone)]
struct Sticker {
    id: &'static str,
    file_id: &'static str,
    unique: String,
    emoji: &'static str,
    animated: bool,
    video: bool,
    repaint: bool,
    bytes: Vec<u8>,
}

struct State {
    stickers: Vec<Sticker>,
    /// 1-based number of the getFile call that answers HTTP 500.
    fail_get_file: Option<usize>,
    get_file_calls: usize,
    requests: usize,
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

/// 100x100 blue disc on a transparent background.
fn disc_webp() -> Vec<u8> {
    let rgba: Vec<u8> = (0..100i32)
        .flat_map(|y| (0..100i32).map(move |x| (x - 50, y - 50)))
        .flat_map(|(dx, dy)| {
            if dx * dx + dy * dy <= 40 * 40 {
                [40, 120, 220, 255]
            } else {
                [0, 0, 0, 0]
            }
        })
        .collect();
    let mut out = Vec::new();
    WebPEncoder::new(&mut out)
        .encode(&rgba, 100, 100, ColorType::Rgba8)
        .unwrap();
    out
}

fn demo_stickers() -> Vec<Sticker> {
    vec![
        Sticker {
            id: ID_TGS,
            file_id: "F-TGS",
            unique: "U-tgs".into(),
            emoji: "😀",
            animated: true,
            video: false,
            repaint: false,
            bytes: gzip(PULSE),
        },
        Sticker {
            id: ID_WEBM,
            file_id: "F-WEBM",
            unique: "U-webm".into(),
            emoji: "🔥",
            animated: false,
            video: true,
            repaint: false,
            bytes: ALPHA_WEBM.to_vec(),
        },
        Sticker {
            id: ID_WEBP,
            file_id: "F-WEBP",
            unique: "U-webp".into(),
            emoji: "⭐",
            animated: false,
            video: false,
            repaint: true,
            bytes: disc_webp(),
        },
    ]
}

fn reply(body: Value) -> (u16, Vec<u8>) {
    (200, body.to_string().into_bytes())
}

/// Answers like the Bot API; the set name is echoed back as requested.
fn respond(state: &mut State, url: &str) -> (u16, Vec<u8>) {
    state.requests += 1;
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let param = |key: &str| {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
            .unwrap_or_default()
            .to_string()
    };
    if path == format!("/bot{TOKEN}/getStickerSet") {
        let stickers: Vec<Value> = state
            .stickers
            .iter()
            .map(|s| {
                json!({
                    "file_id": s.file_id,
                    "file_unique_id": s.unique,
                    "custom_emoji_id": s.id,
                    "emoji": s.emoji,
                    "is_animated": s.animated,
                    "is_video": s.video,
                    "needs_repainting": s.repaint,
                    "type": "custom_emoji",
                })
            })
            .collect();
        return reply(json!({
            "ok": true,
            "result": {
                "name": param("name"),
                "title": "Demo Pack",
                "sticker_type": "custom_emoji",
                "stickers": stickers,
            },
        }));
    }
    if path == format!("/bot{TOKEN}/getFile") {
        state.get_file_calls += 1;
        if state.fail_get_file == Some(state.get_file_calls) {
            let body =
                json!({"ok": false, "error_code": 500, "description": "Internal Server Error"});
            return (500, body.to_string().into_bytes());
        }
        let file_id = param("file_id");
        return reply(json!({
            "ok": true,
            "result": {"file_id": file_id, "file_path": format!("custom_emoji/{file_id}.bin")},
        }));
    }
    let file = path
        .strip_prefix(&format!("/file/bot{TOKEN}/custom_emoji/"))
        .and_then(|rest| rest.strip_suffix(".bin"))
        .and_then(|id| state.stickers.iter().find(|s| s.file_id == id));
    match file {
        Some(sticker) => (200, sticker.bytes.clone()),
        None => (404, br#"{"ok":false,"description":"Not Found"}"#.to_vec()),
    }
}

struct Mock {
    server: Arc<tiny_http::Server>,
    base: String,
    state: Arc<Mutex<State>>,
    thread: Option<JoinHandle<()>>,
}

impl Mock {
    fn start() -> Mock {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        let state = Arc::new(Mutex::new(State {
            stickers: demo_stickers(),
            fail_get_file: None,
            get_file_calls: 0,
            requests: 0,
        }));
        let thread = {
            let server = Arc::clone(&server);
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                for request in server.incoming_requests() {
                    let (status, body) = respond(&mut state.lock().unwrap(), request.url());
                    let response = tiny_http::Response::from_data(body).with_status_code(status);
                    let _ = request.respond(response);
                }
            })
        };
        Mock {
            server,
            base,
            state,
            thread: Some(thread),
        }
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Sandbox {
    root: tempfile::TempDir,
    mock: Mock,
}

impl Sandbox {
    fn new() -> Sandbox {
        Sandbox {
            root: tempfile::tempdir().unwrap(),
            mock: Mock::start(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.mock.state.lock().unwrap()
    }

    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    fn local(&self) -> PathBuf {
        self.home().join("catalogs/local")
    }

    fn pack_dir(&self) -> PathBuf {
        self.local().join("packs/demo")
    }

    fn cached(&self, name: &str) -> PathBuf {
        self.home().join("cache/src").join(name)
    }

    fn command(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::cargo_bin("tge").unwrap();
        cmd.current_dir(self.root.path())
            .env("BOT_TOKEN", TOKEN)
            .env("TGE_BOT_API_BASE", &self.mock.base)
            .env_remove("TG_PREMIUM_EMOJI_HOME")
            .env_remove("TGE_BUILTIN")
            .arg("--home")
            .arg(self.home());
        cmd
    }

    /// `tge --json pack build <name>`; must succeed with exactly one JSON document on stdout.
    fn build(&self, name: &str) -> Value {
        let out = self
            .command()
            .args(["--json", "pack", "build", name])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice(&out).unwrap()
    }

    fn pack(&self) -> Value {
        serde_json::from_slice(&fs::read(self.pack_dir().join("pack.json")).unwrap()).unwrap()
    }
}

fn emoji<'a>(pack: &'a Value, id: &str) -> &'a Value {
    pack["emoji"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("emoji {id} not in pack"))
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn errors_in(dir: &Path) -> Vec<tge::validate::Issue> {
    validate_catalog_dir(dir)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .collect()
}

fn read_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

#[test]
fn first_build_creates_valid_pack() {
    let sandbox = Sandbox::new();
    let doc = sandbox.build("demo");

    assert_eq!(doc["pack"], "demo");
    assert_eq!(doc["report"]["total"], 3);
    assert_eq!(doc["report"]["added"], json!([ID_TGS, ID_WEBM, ID_WEBP]));
    assert_eq!(doc["errors"], json!([]));
    let frames = doc["frames"].as_array().unwrap();
    let frame_ids: Vec<&Value> = frames.iter().map(|f| &f["id"]).collect();
    assert_eq!(frame_ids, [ID_TGS, ID_WEBM, ID_WEBP]);
    for f in frames {
        let (best, first) = (f["best"].as_f64().unwrap(), f["first"].as_f64().unwrap());
        assert!(best > 0.0 && best >= first, "{f}");
        assert!(f["median"].is_number() && f["best_index"].is_u64(), "{f}");
    }

    let issues = validate_catalog_dir(&sandbox.local());
    assert!(issues.is_empty(), "{issues:?}");

    let pack = sandbox.pack();
    assert_eq!(pack["short_name"], "demo");
    assert_eq!(pack["title"], "Demo Pack");
    let rows: Vec<(Value, Value, Value)> = pack["emoji"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["n"].clone(), e["id"].clone(), e["format"].clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (json!(1), json!(ID_TGS), json!("tgs")),
            (json!(2), json!(ID_WEBM), json!("webm")),
            (json!(3), json!(ID_WEBP), json!("static")),
        ]
    );
    assert_eq!(emoji(&pack, ID_WEBP)["repaint"], true);
    assert_eq!(emoji(&pack, ID_TGS)["animated"], true);
    assert_eq!(emoji(&pack, ID_WEBP)["animated"], false);
    let animated: Vec<(&Value, &Value)> =
        frames.iter().map(|f| (&f["id"], &f["animated"])).collect();
    assert_eq!(animated[0], (&json!(ID_TGS), &json!(true)));
    assert_eq!(animated[2], (&json!(ID_WEBP), &json!(false)));
    assert!(emoji(&pack, ID_TGS)["confidence"].is_null());

    let sheet = sandbox.pack_dir().join("preview/demo_1.png");
    assert_eq!(
        file_names(&sandbox.pack_dir().join("preview")),
        ["demo_1.png"]
    );
    let (w, h, px) = read_png(&sheet);
    assert_eq!((w, h), (720, 168));
    assert_eq!(&px[..4], &BG);

    for name in ["U-tgs.tgs", "U-webm.webm", "U-webp.webp"] {
        assert!(sandbox.cached(name).is_file(), "{name} not cached");
    }
}

#[test]
fn rescan_marks_gone() {
    let sandbox = Sandbox::new();
    sandbox.build("demo");
    let mut pack = sandbox.pack();
    pack["emoji"][0]["name"] = json!("pulse");
    pack["emoji"][0]["confidence"] = json!("high");
    fs::write(
        sandbox.pack_dir().join("pack.json"),
        serde_json::to_vec_pretty(&pack).unwrap(),
    )
    .unwrap();

    sandbox.state().stickers.retain(|s| s.id != ID_WEBM);
    let doc = sandbox.build("demo");

    assert_eq!(doc["report"]["gone"], json!([ID_WEBM]));
    assert_eq!(doc["report"]["total"], 2);
    let pack = sandbox.pack();
    let gone = emoji(&pack, ID_WEBM);
    assert_eq!(gone["gone"], true);
    assert!(gone["n"].is_null());
    assert_eq!(emoji(&pack, ID_WEBP)["n"], 2);
    assert_eq!(emoji(&pack, ID_TGS)["name"], "pulse");
    assert_eq!(emoji(&pack, ID_TGS)["confidence"], "high");
    let issues = validate_catalog_dir(&sandbox.local());
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn oversized_set_needs_no_limit() {
    let sandbox = Sandbox::new();
    let disc = demo_stickers().remove(2);
    sandbox.state().stickers = (0..1001u64)
        .map(|i| Sticker {
            id: Box::leak(format!("{}", 5_100_000_000_000_000_000u64 + i).into_boxed_str()),
            file_id: disc.file_id,
            unique: disc.unique.clone(),
            emoji: disc.emoji,
            animated: disc.animated,
            video: disc.video,
            repaint: disc.repaint,
            bytes: disc.bytes.clone(),
        })
        .collect();
    let out = sandbox
        .command()
        .args(["pack", "build", "demo"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("--no-limit"), "{stderr}");
    assert!(!sandbox.pack_dir().join("pack.json").exists());
    assert!(!sandbox.local().exists(), "the catalog was touched");
    assert_eq!(sandbox.state().get_file_calls, 0);

    sandbox
        .command()
        .args(["pack", "build", "demo", "--no-limit"])
        .assert()
        .success();
    assert_eq!(sandbox.pack()["emoji"].as_array().unwrap().len(), 1001);
}

#[test]
fn failed_download_writes_nothing() {
    let sandbox = Sandbox::new();
    sandbox.state().fail_get_file = Some(2);
    let out = sandbox
        .command()
        .args(["pack", "build", "demo"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    for text in [&stderr, &stdout] {
        assert!(!text.contains(TOKEN), "{text}");
        assert!(!text.contains("TEST-fake-token"), "{text}");
    }
    assert!(stderr.contains("HTTP 500"), "{stderr}");

    assert!(!sandbox.pack_dir().join("pack.json").exists());
    assert!(!sandbox.pack_dir().join("preview").exists());
    assert!(!sandbox.local().exists(), "the catalog was touched");
    assert!(sandbox.cached("U-tgs.tgs").is_file());
    assert!(!sandbox.cached("U-webm.webm").exists());
}

#[test]
fn undecodable_source_is_suspect_not_fatal() {
    let sandbox = Sandbox::new();
    sandbox.state().stickers[0].bytes = b"this is not a gzipped tgs".to_vec();
    let doc = sandbox.build("demo");

    let errors = doc["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0]["id"], ID_TGS);
    assert!(errors[0]["error"].as_str().unwrap().contains("gzip"));
    let pack = sandbox.pack();
    assert_eq!(emoji(&pack, ID_TGS)["frame_suspect"], true);
    assert_eq!(emoji(&pack, ID_TGS)["n"], 1);
    assert!(sandbox.pack_dir().join("preview/demo_1.png").is_file());
    let issues = validate_catalog_dir(&sandbox.local());
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn rebuild_with_other_case_updates_same_pack() {
    let sandbox = Sandbox::new();
    sandbox.build("demo");
    // as left by a build killed mid-swap
    let leftover = sandbox.pack_dir().join(".preview-old-1");
    fs::create_dir(&leftover).unwrap();
    fs::write(leftover.join("demo_9.png"), b"stale").unwrap();
    let doc = sandbox.build("DEMO");

    assert_eq!(doc["report"]["added"], json!([]));
    assert_eq!(file_names(&sandbox.local().join("packs")), ["demo"]);
    let pack = sandbox.pack();
    assert_eq!(pack["short_name"], "DEMO");
    assert_eq!(pack["emoji"].as_array().unwrap().len(), 3);
    assert_eq!(
        file_names(&sandbox.pack_dir()),
        ["pack.json", "pack.json.lock", "preview"]
    );
    // the old page is gone with the old preview folder
    assert_eq!(
        file_names(&sandbox.pack_dir().join("preview")),
        ["DEMO_1.png"]
    );
    let errors = errors_in(&sandbox.local());
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn missing_token_is_clear() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command()
        .env_remove("BOT_TOKEN")
        .args(["pack", "build", "demo"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("BOT_TOKEN is not set"), "{stderr}");
    assert_eq!(sandbox.state().requests, 0);
    assert!(!sandbox.local().exists());
}

#[test]
fn unsafe_names_are_rejected_before_writing() {
    let sandbox = Sandbox::new();
    sandbox
        .command()
        .args(["pack", "build", "../x"])
        .assert()
        .code(2);
    assert_eq!(sandbox.state().requests, 0);

    sandbox.state().stickers[1].unique = "../x".into();
    let out = sandbox
        .command()
        .args(["pack", "build", "demo"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("file_unique_id"), "{stderr}");
    assert_eq!(sandbox.state().get_file_calls, 0);
    assert!(!sandbox.home().join("cache").exists());
    assert!(!sandbox.local().exists());
}

#[test]
fn catalog_and_catalog_dir_together_is_usage_error() {
    let sandbox = Sandbox::new();
    let explicit = sandbox.root.path().join("explicit");
    let out = sandbox
        .command()
        .arg("--json")
        .arg("--catalog-dir")
        .arg(&explicit)
        .args(["pack", "build", "demo", "--catalog", "project"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let doc: Value = serde_json::from_slice(&out).unwrap();
    assert!(
        doc["error"].as_str().unwrap().contains("--catalog"),
        "{doc}"
    );
    assert_eq!(sandbox.state().requests, 0);
    assert!(!explicit.exists());
}

#[test]
fn catalog_dir_and_project_targets() {
    let sandbox = Sandbox::new();
    let explicit = sandbox.root.path().join("explicit");
    let out = sandbox
        .command()
        .arg("--catalog-dir")
        .arg(&explicit)
        .args(["pack", "build", "demo"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("pack demo built in "), "{text}");
    assert!(
        text.contains("new 3, changed 0, gone 0, returned 0, total 3"),
        "{text}"
    );
    assert!(explicit.join("packs/demo/preview/demo_1.png").is_file());

    sandbox
        .command()
        .args(["pack", "build", "demo", "--catalog", "project"])
        .assert()
        .success();
    let project = sandbox.root.path().join(".tg-premium-emoji");
    assert!(project.join("packs/demo/pack.json").is_file());
    assert!(!sandbox.local().exists());
    for dir in [explicit, project] {
        let issues = validate_catalog_dir(&dir);
        assert!(issues.is_empty(), "{issues:?}");
    }
}

#[test]
fn rebuild_of_a_builtin_only_pack_keeps_its_labels() {
    let sandbox = Sandbox::new();
    sandbox.build("demo");
    let mut pack = sandbox.pack();
    pack["emoji"][0]["name"] = json!("disc");
    pack["emoji"][0]["confidence"] = json!("high");
    pack["emoji"][0]["by"] = json!("human");
    fs::write(
        sandbox.pack_dir().join("pack.json"),
        serde_json::to_string_pretty(&pack).unwrap(),
    )
    .unwrap();
    let labeled_id = pack["emoji"][0]["id"].as_str().unwrap().to_string();
    let builtin = sandbox.root.path().join("builtin");
    fs::rename(sandbox.local(), &builtin).unwrap();

    let assert = sandbox
        .command()
        .arg("--builtin")
        .arg(&builtin)
        .args(["--json", "pack", "build", "demo"])
        .assert()
        .success();
    let output = assert.get_output().clone();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(doc["seeded_from"].is_string(), "{doc}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("starting from labels in catalog"),
        "{stderr}"
    );
    let local = emoji(&sandbox.pack(), &labeled_id).clone();
    assert_eq!(local["name"], "disc");
    assert_eq!(local["by"], "human");
}
