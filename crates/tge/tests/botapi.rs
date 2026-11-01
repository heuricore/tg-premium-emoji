use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tge::botapi::{BotApi, StickerSet, redact};
use tge::merge::FetchedEmoji;
use tge::model::pack::Format;

const TOKEN: &str = "123456:TEST-fake-token";

const SET_FIXTURE: &str = include_str!("fixtures/botapi/sticker_set.json");
const REGULAR_FIXTURE: &str = include_str!("fixtures/botapi/regular_set.json");

type Handler = dyn Fn(&str) -> (u16, Vec<u8>) + Send + Sync;

struct Mock {
    server: Arc<tiny_http::Server>,
    base: String,
    seen: Arc<Mutex<Vec<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl Mock {
    fn start(handler: impl Fn(&str) -> (u16, Vec<u8>) + Send + Sync + 'static) -> Mock {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let handler: Arc<Handler> = Arc::new(handler);
        let thread = {
            let server = Arc::clone(&server);
            let seen = Arc::clone(&seen);
            std::thread::spawn(move || {
                for request in server.incoming_requests() {
                    let url = request.url().to_string();
                    seen.lock().unwrap().push(url.clone());
                    let (status, body) = handler(&url);
                    let response = tiny_http::Response::from_data(body).with_status_code(status);
                    let _ = request.respond(response);
                }
            })
        };
        Mock {
            server,
            base,
            seen,
            thread: Some(thread),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }

    fn api(&self) -> BotApi {
        api_for(&self.base)
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

fn api_for(base: &str) -> BotApi {
    let base = base.to_string();
    BotApi::from_lookup(move |key| match key {
        "BOT_TOKEN" => Some(TOKEN.to_string()),
        "TGE_BOT_API_BASE" => Some(base.clone()),
        _ => None,
    })
    .unwrap()
}

fn parse_set(fixture: &str) -> StickerSet {
    let value: serde_json::Value = serde_json::from_str(fixture).unwrap();
    serde_json::from_value(value["result"].clone()).unwrap()
}

fn set_with(stickers: serde_json::Value, sticker_type: &str) -> StickerSet {
    serde_json::from_value(serde_json::json!({
        "name": "demo",
        "title": "Demo",
        "sticker_type": sticker_type,
        "stickers": stickers,
    }))
    .unwrap()
}

fn good_sticker() -> serde_json::Value {
    serde_json::json!({
        "file_id": "F",
        "file_unique_id": "U",
        "custom_emoji_id": "123",
        "emoji": "x",
    })
}

#[test]
fn fetched_mapping() {
    let fetched = parse_set(SET_FIXTURE).into_fetched().unwrap();
    assert_eq!(
        fetched,
        vec![
            FetchedEmoji {
                n: 1,
                id: "5368324170671202286".into(),
                file_unique_id: "UNIQ-A".into(),
                fallback: "😀".into(),
                format: Format::Tgs,
                repaint: false,
                file_id: "FILE-A".into(),
            },
            FetchedEmoji {
                n: 2,
                id: "5368324170671202287".into(),
                file_unique_id: "UNIQ-B".into(),
                fallback: "🔥".into(),
                format: Format::Webm,
                repaint: false,
                file_id: "FILE-B".into(),
            },
            FetchedEmoji {
                n: 3,
                id: "5368324170671202288".into(),
                file_unique_id: "UNIQ-C".into(),
                fallback: "⭐".into(),
                format: Format::Static,
                repaint: true,
                file_id: "FILE-C".into(),
            },
        ]
    );
}

#[test]
fn regular_set_is_rejected() {
    let err = parse_set(REGULAR_FIXTURE).into_fetched().unwrap_err();
    let text = err.to_string();
    assert!(text.contains("plain_stickers"), "{text}");
    assert!(text.contains("regular"), "{text}");
}

#[test]
fn bad_stickers_are_rejected_with_position() {
    let mut no_id = good_sticker();
    no_id.as_object_mut().unwrap().remove("custom_emoji_id");
    let err = set_with(serde_json::json!([good_sticker(), no_id]), "custom_emoji")
        .into_fetched()
        .unwrap_err();
    assert!(err.to_string().contains("sticker 2"), "{err}");

    let mut bad_id = good_sticker();
    bad_id["custom_emoji_id"] = "12ab".into();
    let err = set_with(serde_json::json!([bad_id]), "custom_emoji")
        .into_fetched()
        .unwrap_err();
    assert!(err.to_string().contains("sticker 1"), "{err}");

    let mut long_id = good_sticker();
    long_id["custom_emoji_id"] = "1".repeat(21).into();
    assert!(
        set_with(serde_json::json!([long_id]), "custom_emoji")
            .into_fetched()
            .is_err()
    );

    let mut no_emoji = good_sticker();
    no_emoji["emoji"] = "".into();
    let err = set_with(serde_json::json!([no_emoji]), "custom_emoji")
        .into_fetched()
        .unwrap_err();
    assert!(err.to_string().contains("sticker 1"), "{err}");

    let mut missing_emoji = good_sticker();
    missing_emoji.as_object_mut().unwrap().remove("emoji");
    assert!(
        set_with(serde_json::json!([missing_emoji]), "custom_emoji")
            .into_fetched()
            .is_err()
    );
}

#[test]
fn more_than_200_stickers_are_rejected() {
    let many: Vec<_> = (0..201).map(|_| good_sticker()).collect();
    assert!(
        set_with(serde_json::Value::Array(many), "custom_emoji")
            .into_fetched()
            .is_err()
    );
    let ok: Vec<_> = (0..200).map(|_| good_sticker()).collect();
    assert_eq!(
        set_with(serde_json::Value::Array(ok), "custom_emoji")
            .into_fetched()
            .unwrap()
            .len(),
        200
    );
}

#[test]
fn redact_hides_token_in_url() {
    let text = format!("GET https://api.telegram.org/bot{TOKEN}/getFile failed; again {TOKEN}");
    let out = redact(&text, TOKEN);
    assert!(!out.contains(TOKEN), "{out}");
    assert_eq!(
        out,
        "GET https://api.telegram.org/bot<BOT_TOKEN>/getFile failed; again <BOT_TOKEN>"
    );
    assert_eq!(redact("nothing here", ""), "nothing here");
}

#[test]
fn missing_token_message() {
    for value in [None, Some(""), Some("   ")] {
        let result = BotApi::from_lookup(|key| match key {
            "BOT_TOKEN" => value.map(str::to_string),
            _ => None,
        });
        let err = result.expect_err("must fail without a token");
        assert_eq!(err.to_string(), "BOT_TOKEN is not set");
    }
}

#[test]
fn debug_output_hides_token() {
    let api = api_for("http://127.0.0.1:1");
    let text = format!("{api:?}");
    assert!(!text.contains(TOKEN), "{text}");
    assert!(text.contains("<BOT_TOKEN>"), "{text}");
}

#[test]
fn trailing_slash_on_base_is_trimmed() {
    let mock = Mock::start(|_| (200, SET_FIXTURE.as_bytes().to_vec()));
    let api = api_for(&format!("{}/", mock.base));
    api.get_sticker_set("demo_pack").unwrap();
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].starts_with(&format!("/bot{TOKEN}/getStickerSet?")),
        "{requests:?}"
    );
}

#[test]
fn set_name_is_url_encoded() {
    let mock = Mock::start(|_| (200, SET_FIXTURE.as_bytes().to_vec()));
    mock.api().get_sticker_set("a b&c=d").unwrap();
    let requests = mock.requests();
    assert!(requests[0].ends_with("?name=a%20b%26c%3Dd"), "{requests:?}");
}

#[test]
fn gets_sticker_set() {
    let mock = Mock::start(|_| (200, SET_FIXTURE.as_bytes().to_vec()));
    let set = mock.api().get_sticker_set("demo_pack").unwrap();
    assert_eq!(set.name, "demo_pack");
    assert_eq!(set.stickers.len(), 3);
}

#[test]
fn download_fetches_file_bytes() {
    let mock = Mock::start(|url| {
        if url.contains("/getFile") {
            let body =
                r#"{"ok":true,"result":{"file_id":"F","file_path":"custom_emoji/file_1.tgs"}}"#;
            (200, body.as_bytes().to_vec())
        } else {
            (200, b"BYTES".to_vec())
        }
    });
    let bytes = mock.api().download("F").unwrap();
    assert_eq!(bytes, b"BYTES");
    let requests = mock.requests();
    assert!(requests[0].contains("file_id=F"), "{requests:?}");
    assert_eq!(
        requests[1],
        format!("/file/bot{TOKEN}/custom_emoji/file_1.tgs")
    );
}

const MAX_DOWNLOAD: usize = 20 * 1024 * 1024;

fn file_mock(file_body: impl Fn() -> Vec<u8> + Send + Sync + 'static) -> Mock {
    Mock::start(move |url| {
        if url.contains("/getFile") {
            let body =
                r#"{"ok":true,"result":{"file_id":"F","file_path":"custom_emoji/file_1.tgs"}}"#;
            (200, body.as_bytes().to_vec())
        } else {
            (200, file_body())
        }
    })
}

#[test]
fn download_cap_is_inclusive() {
    let exact = file_mock(|| vec![7u8; MAX_DOWNLOAD]);
    assert_eq!(exact.api().download("F").unwrap().len(), MAX_DOWNLOAD);
    let over = file_mock(|| vec![7u8; MAX_DOWNLOAD + 1]);
    assert!(over.api().download("F").is_err());
}

#[test]
fn gzip_is_not_requested_and_raw_bytes_are_capped() {
    let headers = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&headers);
    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let worker = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                for h in request.headers() {
                    if h.field.equiv("Accept-Encoding") {
                        seen.lock().unwrap().push(h.value.to_string());
                    }
                }
                let body = if request.url().contains("/getFile") {
                    br#"{"ok":true,"result":{"file_path":"a/b.bin"}}"#.to_vec()
                } else {
                    // gzip magic followed by junk, larger than the cap when counted raw
                    let mut raw = vec![0x1f, 0x8b, 0x08];
                    raw.resize(MAX_DOWNLOAD + 1, 0);
                    raw
                };
                let response = tiny_http::Response::from_data(body).with_header(
                    "Content-Encoding: gzip"
                        .parse::<tiny_http::Header>()
                        .unwrap(),
                );
                let _ = request.respond(response);
            }
        })
    };
    let result = api_for(&base).download("F");
    server.unblock();
    worker.join().unwrap();
    assert!(result.is_err());
    assert!(
        headers.lock().unwrap().is_empty(),
        "Accept-Encoding sent: {:?}",
        headers.lock().unwrap()
    );
}

#[test]
fn retries_after_429() {
    let calls = Arc::new(Mutex::new(0u32));
    let counter = Arc::clone(&calls);
    let mock = Mock::start(move |_| {
        let mut n = counter.lock().unwrap();
        *n += 1;
        if *n == 1 {
            let body = r#"{"ok":false,"error_code":429,"description":"Too Many Requests: retry after 1","parameters":{"retry_after":1}}"#;
            (429, body.as_bytes().to_vec())
        } else {
            (200, SET_FIXTURE.as_bytes().to_vec())
        }
    });
    let set = mock.api().get_sticker_set("demo_pack").unwrap();
    assert_eq!(set.stickers.len(), 3);
    assert_eq!(mock.requests().len(), 2);
}

#[test]
fn long_retry_after_is_an_error_without_sleeping() {
    let mock = Mock::start(|_| {
        let body = r#"{"ok":false,"error_code":429,"description":"Too Many Requests: retry after 120","parameters":{"retry_after":120}}"#;
        (429, body.as_bytes().to_vec())
    });
    let started = Instant::now();
    let err = mock.api().get_sticker_set("demo_pack").unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(err.to_string().contains("120"), "{err}");
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn http_error_is_redacted() {
    let mock = Mock::start(|url| {
        if url.contains("/getFile") {
            let body =
                r#"{"ok":true,"result":{"file_id":"F","file_path":"custom_emoji/file_1.tgs"}}"#;
            (200, body.as_bytes().to_vec())
        } else {
            (500, format!("internal error at {url}").into_bytes())
        }
    });
    let err = mock.api().download("F").unwrap_err();
    let text = err.to_string();
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!text.contains("TEST-fake-token"), "{text}");
}

#[test]
fn api_error_is_redacted() {
    let mock = Mock::start(|url| {
        let body = serde_json::json!({
            "ok": false,
            "error_code": 400,
            "description": format!("Bad Request: cannot handle {url}"),
        });
        (400, body.to_string().into_bytes())
    });
    let err = mock.api().get_sticker_set("demo_pack").unwrap_err();
    let text = err.to_string();
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!text.contains("TEST-fake-token"), "{text}");
    assert!(text.contains("Bad Request"), "{text}");
}

#[test]
fn unreachable_server_error_is_redacted() {
    let api = api_for("http://127.0.0.1:1");
    let err = api.get_sticker_set("demo_pack").unwrap_err();
    let text = err.to_string();
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!text.contains("TEST-fake-token"), "{text}");
}
