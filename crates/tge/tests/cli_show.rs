use std::fs;
use std::path::Path;

use serde_json::Value;

const ID1: &str = "5000000000000000001";
const ID_GONE: &str = "5000000000000000004";

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

    fn command(&self, json: bool, args: &[&str]) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::cargo_bin("tge").unwrap();
        cmd.current_dir(self.home.path());
        if json {
            cmd.arg("--json");
        }
        cmd.arg("--home")
            .arg(self.home.path().join("state"))
            .arg("--catalog-dir")
            .arg(self.catalog.path())
            .args(args);
        cmd
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self
            .command(true, args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice(&out).unwrap()
    }
}

#[test]
fn show_by_n_has_every_field() {
    let sandbox = Sandbox::new();
    let doc = sandbox.json(&["show", "demo", "2"]);
    let docs = doc.as_array().unwrap();
    assert_eq!(docs.len(), 1);
    let e = docs[0].as_object().unwrap();
    for key in [
        "n",
        "id",
        "file_unique_id",
        "fallback",
        "format",
        "animated",
        "repaint",
        "name",
        "description",
        "tags",
        "fits",
        "confidence",
        "by",
        "frame_suspect",
        "gone",
        "catalog",
        "catalog_role",
        "pack",
        "pack_title",
        "sheet",
        "page",
        "row",
        "column",
        "html",
        "markdown_v2",
        "button",
    ] {
        assert!(e.contains_key(key), "missing {key}");
    }
    assert_eq!(e["n"], 2);
    assert_eq!(e["format"], "tgs");
    assert_eq!(e["animated"], true);
    assert_eq!(e["fits"][0], "button");
    assert_eq!(e["catalog_role"], "explicit");
    assert_eq!(e["pack"], "demo");
    assert_eq!(e["pack_title"], "Demo pack");
    assert_eq!(
        (e["page"].as_u64(), e["row"].as_u64(), e["column"].as_u64()),
        (Some(1), Some(1), Some(2))
    );
    let sheet = e["sheet"].as_str().unwrap();
    assert!(
        Path::new(sheet).is_absolute() && sheet.ends_with("demo_1.png"),
        "{sheet}"
    );
    assert_eq!(e["button"]["text"], "LABEL");
}

#[test]
fn show_by_id_matches_by_n_and_shows_gone() {
    let sandbox = Sandbox::new();
    assert_eq!(
        sandbox.json(&["show", "demo", "1"]),
        sandbox.json(&["show", "demo", ID1])
    );
    let doc = sandbox.json(&["show", "demo", ID_GONE]);
    assert_eq!(doc[0]["gone"], true);
    assert!(doc[0]["n"].is_null() && doc[0]["sheet"].is_null() && doc[0]["page"].is_null());
    assert_eq!(doc[0]["by"], "human");
}

#[test]
fn show_two_targets_in_order() {
    let doc = Sandbox::new().json(&["show", "demo", "3", "1"]);
    let ns: Vec<_> = doc
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["n"].clone())
        .collect();
    assert_eq!(ns, [3, 1]);
}

#[test]
fn show_human_prints_dashes_for_empty_values() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command(false, &["show", "demo", "3", "1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("name: -\n") && text.contains("tags: -\n"),
        "{text}"
    );
    assert!(text.contains("tags: галочка, check, done\n"), "{text}");
    assert!(text.matches("\nn: ").count() + 1 >= 2, "{text}");
}

#[test]
fn show_unknown_target_prints_nothing_and_exits_1() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command(false, &["show", "demo", "1", "99"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("99"));
    sandbox
        .command(true, &["show", "nope", "1"])
        .assert()
        .code(1);
}
