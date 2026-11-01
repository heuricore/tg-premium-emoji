use std::fs;
use std::path::Path;

use serde_json::Value;
use tge::snippet::tg_emoji_html;

const ID1: &str = "5000000000000000001";

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Sandbox {
    catalog: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Sandbox {
        let catalog = tempfile::tempdir().unwrap();
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalog-search"),
            catalog.path(),
        );
        Sandbox {
            catalog,
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn cmd(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        assert_cmd::Command::cargo_bin("tge")
            .unwrap()
            .current_dir(self.home.path())
            .arg("--json")
            .arg("--home")
            .arg(self.home.path().join("state"))
            .arg("--catalog-dir")
            .arg(self.catalog.path())
            .args(args)
            .assert()
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self.cmd(args).success().get_output().stdout.clone();
        serde_json::from_slice(&out).unwrap()
    }
}

#[test]
fn find_by_russian_tag() {
    let doc = Sandbox::new().json(&["find", "галочк"]);
    let hit = &doc["hits"][0];
    assert_eq!(hit["n"], 1);
    assert_eq!(hit["id"], ID1);
    assert_eq!(hit["pack"], "demo");
    assert_eq!(hit["html"], tg_emoji_html(ID1, "✅"));
}

#[test]
fn find_reports_unlabeled() {
    let doc = Sandbox::new().json(&["find", "check"]);
    assert_eq!(doc["unlabeled"][0]["pack"], "demo");
    assert_eq!(doc["unlabeled"][0]["count"], 1);
}

#[test]
fn find_human_output_lists_unlabeled() {
    let sandbox = Sandbox::new();
    let out = assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .current_dir(sandbox.home.path())
        .args(["--home"])
        .arg(sandbox.home.path().join("state"))
        .arg("--catalog-dir")
        .arg(sandbox.catalog.path())
        .args(["find", "check"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("pack demo: 1 unlabeled"), "{text}");
    assert!(text.contains("demo #1 ✅ check mark (high)"), "{text}");
}

#[test]
fn find_never_returns_gone() {
    let doc = Sandbox::new().json(&["find", "old"]);
    assert!(doc["hits"].as_array().unwrap().is_empty());
}

#[test]
fn find_filters_by_fits() {
    let sandbox = Sandbox::new();
    let doc = sandbox.json(&["find", "cancel", "--fits", "button"]);
    assert_eq!(doc["hits"][0]["n"], 2);
    let doc = sandbox.json(&["find", "cancel", "--fits", "heading"]);
    assert!(doc["hits"].as_array().unwrap().is_empty());
}

#[test]
fn find_filters_by_repaint_and_format() {
    let sandbox = Sandbox::new();
    let doc = sandbox.json(&["find", "check", "cross", "--repaint", "true"]);
    let ns: Vec<_> = doc["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| &h["n"])
        .collect();
    assert_eq!(ns, [2]);
    let doc = sandbox.json(&["find", "check", "cross", "--format", "static"]);
    let ns: Vec<_> = doc["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| &h["n"])
        .collect();
    assert_eq!(ns, [1]);
}

#[test]
fn find_rejects_query_without_words() {
    let out = Sandbox::new()
        .cmd(&["find", "!!!"])
        .code(2)
        .get_output()
        .stdout
        .clone();
    let doc: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(doc["error"], "query has no searchable words");
}

#[test]
fn find_rejects_query_without_words_in_human_mode() {
    let sandbox = Sandbox::new();
    let err = assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .current_dir(sandbox.home.path())
        .arg("--home")
        .arg(sandbox.home.path().join("state"))
        .arg("--catalog-dir")
        .arg(sandbox.catalog.path())
        .args(["find", "!!!"])
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8(err).unwrap();
    assert!(text.contains("query has no searchable words"), "{text}");
}

#[test]
fn snippet_by_n_and_by_id_agree() {
    let sandbox = Sandbox::new();
    let by_n = sandbox.json(&["snippet", "demo", "1"]);
    let by_id = sandbox.json(&["snippet", "demo", ID1]);
    assert_eq!(by_n, by_id);
    assert_eq!(by_n["html"], tg_emoji_html(ID1, "✅"));
    assert_eq!(by_n["markdown_v2"], format!("![✅](tg://emoji?id={ID1})"));
    assert_eq!(by_n["button"]["text"], "LABEL");
    let custom = sandbox.json(&["snippet", "demo", "1", "--text", "Save"]);
    assert_eq!(custom["button"]["text"], "Save");
}

#[test]
fn snippet_unknown_target_is_runtime_error() {
    let sandbox = Sandbox::new();
    sandbox.cmd(&["snippet", "nope", "1"]).code(1);
    sandbox.cmd(&["snippet", "demo", "99"]).code(1);
}
