use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

const ID1: &str = "5000000000000000001";
const ID4: &str = "5000000000000000004";

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

    fn command(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::cargo_bin("tge").unwrap();
        cmd.current_dir(self.home.path())
            .arg("--home")
            .arg(self.home.path().join("state"));
        cmd
    }

    fn cmd(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        self.command()
            .arg("--json")
            .arg("--catalog-dir")
            .arg(self.catalog.path())
            .args(args)
            .assert()
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self.cmd(args).success().get_output().stdout.clone();
        serde_json::from_slice(&out).unwrap()
    }

    fn pack_path(&self) -> PathBuf {
        self.catalog.path().join("packs/demo/pack.json")
    }

    fn pack_text(&self) -> String {
        fs::read_to_string(self.pack_path()).unwrap()
    }

    fn pack(&self) -> Value {
        serde_json::from_str(&self.pack_text()).unwrap()
    }

    fn edit_pack(&self, f: impl FnOnce(&mut Value)) {
        let mut pack = self.pack();
        f(&mut pack);
        fs::write(
            self.pack_path(),
            serde_json::to_string_pretty(&pack).unwrap(),
        )
        .unwrap();
    }

    fn label(&self, edits: &Value, extra: &[&str]) -> assert_cmd::assert::Assert {
        let file = self.home.path().join("edits.json");
        fs::write(&file, edits.to_string()).unwrap();
        let mut args = vec!["label", "demo", "--edits", file.to_str().unwrap()];
        args.extend_from_slice(extra);
        self.cmd(&args)
    }
}

fn stdout_json(assert: assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assert.get_output().stdout).unwrap()
}

#[test]
fn label_partial_update() {
    let sandbox = Sandbox::new();
    let doc = stdout_json(
        sandbox
            .label(&json!([{ "n": 2, "name": "red cross" }]), &[])
            .success(),
    );
    assert_eq!(doc["applied"], json!(["2"]));
    let emoji = &sandbox.pack()["emoji"][1];
    assert_eq!(emoji["name"], "red cross");
    assert_eq!(emoji["tags"], json!(["крестик", "cancel"]));
    assert_eq!(emoji["fits"], json!(["button"]));
    assert_eq!(emoji["confidence"], "medium");
    assert_eq!(emoji["by"], "agent");
    assert_eq!(sandbox.pack()["emoji"][0]["name"], "check mark");
}

#[test]
fn null_clears_description() {
    let sandbox = Sandbox::new();
    sandbox
        .label(
            &json!([{ "n": 1, "description": null }]),
            &["--by", "human"],
        )
        .success();
    let emoji = &sandbox.pack()["emoji"][0];
    assert_eq!(emoji["description"], Value::Null);
    assert_eq!(emoji["name"], "check mark");
    assert_eq!(emoji["by"], "human");
}

#[test]
fn name_null_clears_name() {
    let sandbox = Sandbox::new();
    sandbox
        .label(&json!([{ "n": 2, "name": null }]), &[])
        .success();
    assert_eq!(sandbox.pack()["emoji"][1]["name"], Value::Null);
}

#[test]
fn agent_cannot_override_human() {
    let sandbox = Sandbox::new();
    let doc = stdout_json(
        sandbox
            .label(&json!([{ "n": 1, "name": "tick" }]), &[])
            .success(),
    );
    assert_eq!(doc["skipped_human"], json!(["1"]));
    assert_eq!(doc["applied"], json!([]));
    assert_eq!(sandbox.pack()["emoji"][0]["name"], "check mark");
    assert_eq!(sandbox.pack()["emoji"][0]["by"], "human");
}

#[test]
fn force_overrides_human() {
    let sandbox = Sandbox::new();
    let doc = stdout_json(
        sandbox
            .label(&json!([{ "n": 1, "name": "tick" }]), &["--force"])
            .success(),
    );
    assert_eq!(doc["applied"], json!(["1"]));
    assert_eq!(sandbox.pack()["emoji"][0]["name"], "tick");
    assert_eq!(sandbox.pack()["emoji"][0]["by"], "agent");
}

#[test]
fn n_and_id_together_is_usage_error() {
    let sandbox = Sandbox::new();
    let before = sandbox.pack_text();
    let doc = stdout_json(
        sandbox
            .label(&json!([{ "n": 2, "id": ID1, "name": "x" }]), &[])
            .code(2),
    );
    assert!(doc["error"].as_str().unwrap().contains("exactly one"));
    sandbox.label(&json!([{ "name": "x" }]), &[]).code(2);
    assert_eq!(sandbox.pack_text(), before);
}

#[test]
fn malformed_edits_are_usage_errors() {
    let sandbox = Sandbox::new();
    let before = sandbox.pack_text();
    sandbox
        .label(&json!([{ "n": 2, "colour": "red" }]), &[])
        .code(2);
    sandbox
        .label(&json!([{ "n": 2, "confidence": "certain" }]), &[])
        .code(2);
    sandbox
        .label(&json!([{ "n": 2, "tags": null }]), &[])
        .code(2);
    sandbox.label(&json!({ "n": 2 }), &[]).code(2);
    assert_eq!(sandbox.pack_text(), before);
}

#[test]
fn unknown_n_is_reported() {
    let sandbox = Sandbox::new();
    let doc = stdout_json(
        sandbox
            .label(
                &json!([{ "n": 99, "name": "x", "confidence": "high" }, { "id": "777", "name": "y" }]),
                &[],
            )
            .success(),
    );
    assert_eq!(doc["not_found"], json!(["99", "777"]));
    assert_eq!(doc["applied"], json!([]));
}

#[test]
fn edit_by_id_reaches_gone_emoji_but_n_does_not() {
    let sandbox = Sandbox::new();
    let doc = stdout_json(
        sandbox
            .label(
                &json!([{ "id": ID4, "tags": ["old", "legacy"] }, { "n": 4, "name": "x" }]),
                &["--by", "human"],
            )
            .success(),
    );
    assert_eq!(doc["applied"], json!([ID4]));
    assert_eq!(doc["not_found"], json!(["4"]));
    assert_eq!(sandbox.pack()["emoji"][3]["tags"], json!(["old", "legacy"]));
}

#[test]
fn too_long_name_writes_nothing() {
    let sandbox = Sandbox::new();
    let before = sandbox.pack_text();
    let long = "x".repeat(65);
    let doc = stdout_json(
        sandbox
            .label(&json!([{ "n": 2, "name": long }]), &[])
            .code(3),
    );
    assert!(!doc["issues"].as_array().unwrap().is_empty());
    assert_eq!(sandbox.pack_text(), before);
}

#[test]
fn missing_confidence_on_new_label_is_invalid() {
    let sandbox = Sandbox::new();
    let before = sandbox.pack_text();
    let doc = stdout_json(
        sandbox
            .label(&json!([{ "n": 3, "name": "star" }]), &[])
            .code(3),
    );
    assert!(
        doc["issues"][0]["message"]
            .as_str()
            .unwrap()
            .contains("confidence")
    );
    assert_eq!(sandbox.pack_text(), before);
    sandbox
        .label(
            &json!([{ "n": 3, "name": "star", "confidence": "high" }]),
            &[],
        )
        .success();
    assert_eq!(sandbox.pack()["emoji"][2]["name"], "star");
}

#[test]
fn edits_from_stdin() {
    let sandbox = Sandbox::new();
    sandbox
        .command()
        .arg("--json")
        .arg("--catalog-dir")
        .arg(sandbox.catalog.path())
        .args(["label", "demo", "--edits", "-"])
        .write_stdin(json!([{ "n": 2, "name": "from stdin" }]).to_string())
        .assert()
        .success();
    assert_eq!(sandbox.pack()["emoji"][1]["name"], "from stdin");
}

#[test]
fn label_refuses_builtin() {
    let sandbox = Sandbox::new();
    let before = sandbox.pack_text();
    let file = sandbox.home.path().join("edits.json");
    fs::write(&file, json!([{ "n": 2, "name": "x" }]).to_string()).unwrap();
    let args = ["label", "demo", "--edits", file.to_str().unwrap()];
    let assert = sandbox
        .command()
        .arg("--json")
        .arg("--builtin")
        .arg(sandbox.catalog.path())
        .args(args)
        .assert()
        .code(1);
    let doc = stdout_json(assert);
    assert!(doc["error"].as_str().unwrap().contains("read-only"));
    assert_eq!(sandbox.pack_text(), before);

    // the same directory given as --catalog-dir is still the built-in catalog
    sandbox
        .command()
        .arg("--json")
        .arg("--builtin")
        .arg(sandbox.catalog.path())
        .arg("--catalog-dir")
        .arg(sandbox.catalog.path())
        .args(args)
        .assert()
        .code(1);
    assert_eq!(sandbox.pack_text(), before);
}

#[test]
fn label_writes_a_copy_of_the_builtin_catalog() {
    let sandbox = Sandbox::new();
    let copy = sandbox.home.path().join("copy");
    copy_dir(sandbox.catalog.path(), &copy);
    let file = sandbox.home.path().join("edits.json");
    fs::write(&file, json!([{ "n": 2, "name": "x" }]).to_string()).unwrap();
    sandbox
        .command()
        .arg("--json")
        .arg("--builtin")
        .arg(sandbox.catalog.path())
        .arg("--catalog-dir")
        .arg(&copy)
        .args(["label", "demo", "--edits", file.to_str().unwrap()])
        .assert()
        .success();
    let written: Value =
        serde_json::from_str(&fs::read_to_string(copy.join("packs/demo/pack.json")).unwrap())
            .unwrap();
    assert_eq!(written["emoji"][1]["name"], "x");
}

#[test]
fn label_rejects_an_edit_with_only_a_selector() {
    let sandbox = Sandbox::new();
    let before = sandbox.pack_text();
    let file = sandbox.home.path().join("edits.json");
    fs::write(&file, json!([{ "n": 2 }]).to_string()).unwrap();
    sandbox
        .cmd(&["label", "demo", "--edits", file.to_str().unwrap()])
        .code(2);
    assert_eq!(sandbox.pack_text(), before);
}

#[test]
fn unknown_pack_is_runtime_error() {
    let sandbox = Sandbox::new();
    let file = sandbox.home.path().join("empty.json");
    fs::write(&file, "[]").unwrap();
    sandbox
        .cmd(&["label", "nope", "--edits", file.to_str().unwrap()])
        .code(1);
    sandbox.cmd(&["report", "nope"]).code(1);
    sandbox.cmd(&["similar", "nope"]).code(1);
    sandbox.cmd(&["pack", "preview", "nope"]).code(1);
}

#[test]
fn similar_prints_groups() {
    let sandbox = Sandbox::new();
    sandbox
        .label(
            &json!([{ "n": 3, "name": "Cross", "confidence": "low" }]),
            &[],
        )
        .success();
    let doc = sandbox.json(&["similar", "demo"]);
    assert_eq!(doc[0][0]["n"], 2);
    assert_eq!(doc[0][0]["name"], "cross");
    assert_eq!(doc[0][1]["n"], 3);
    assert_eq!(doc[0][1]["fallback"], "⭐");
    assert_eq!(doc.as_array().unwrap().len(), 1);
}

#[test]
fn report_lists_queues() {
    let sandbox = Sandbox::new();
    let doc = sandbox.json(&["report", "demo"]);
    assert_eq!(doc["pack"], "demo");
    assert_eq!(doc["total"], 3);
    assert_eq!(doc["unlabeled"], json!([3]));
    assert_eq!(doc["low"], json!([]));
    assert_eq!(doc["frame_suspect"], json!([]));
    assert_eq!(doc["gone"], json!([ID4]));

    sandbox.edit_pack(|pack| {
        pack["emoji"][1]["confidence"] = json!("low");
        pack["emoji"][1]["frame_suspect"] = json!(true);
    });
    let doc = sandbox.json(&["report", "demo"]);
    assert_eq!(doc["low"], json!([2]));
    assert_eq!(doc["frame_suspect"], json!([2]));

    let all = sandbox.json(&["report"]);
    assert_eq!(all.as_array().unwrap().len(), 1);
    assert_eq!(all[0]["pack"], "demo");
}

#[test]
fn report_human_output_lists_queues() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command()
        .arg("--catalog-dir")
        .arg(sandbox.catalog.path())
        .args(["report", "demo"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("unlabeled: 3"), "{text}");
    assert!(text.contains(&format!("gone: {ID4}")), "{text}");
}

fn big_pack(count: u32) -> Value {
    let emoji: Vec<Value> = (1..=count)
        .map(|n| {
            json!({
                "n": n, "id": format!("{}", 6000 + n), "file_unique_id": format!("U{n}"),
                "fallback": "x", "format": "static", "repaint": false, "name": null,
                "description": null, "tags": [], "confidence": null, "frame_suspect": false
            })
        })
        .collect();
    json!({
        "schema_version": 1, "short_name": "big", "title": "Big",
        "fetched_at": "2026-11-01T00:00:00Z",
        "preview": { "cell": 144, "cols": 5, "per_sheet": 20 },
        "emoji": emoji
    })
}

#[test]
fn preview_lists_pages_with_ranges() {
    let sandbox = Sandbox::new();
    let dir = sandbox.catalog.path().join("packs/Big");
    fs::create_dir_all(dir.join("preview")).unwrap();
    fs::write(
        dir.join("pack.json"),
        serde_json::to_string(&big_pack(25)).unwrap(),
    )
    .unwrap();
    let doc_before = sandbox.cmd(&["pack", "preview", "big"]).code(1);
    let err = stdout_json(doc_before);
    assert!(err["error"].as_str().unwrap().contains("tge pack build"));

    fs::write(dir.join("preview/big_1.png"), b"png").unwrap();
    fs::write(dir.join("preview/big_2.png"), b"png").unwrap();
    let doc = sandbox.json(&["pack", "preview", "BIG"]);
    assert_eq!(doc["pack"], "big");
    let sheets = doc["sheets"].as_array().unwrap();
    assert_eq!(sheets.len(), 2);
    assert_eq!(sheets[0]["page"], 1);
    assert_eq!(
        (sheets[0]["from"].as_u64(), sheets[0]["to"].as_u64()),
        (Some(1), Some(20))
    );
    assert_eq!(
        (sheets[1]["from"].as_u64(), sheets[1]["to"].as_u64()),
        (Some(21), Some(25))
    );
    let path = Path::new(sheets[1]["path"].as_str().unwrap());
    assert!(path.is_absolute());
    assert!(path.ends_with("packs/Big/preview/big_2.png"));

    let out = sandbox
        .command()
        .arg("--catalog-dir")
        .arg(sandbox.catalog.path())
        .args(["pack", "preview", "big"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].ends_with("big_1.png  1-20"), "{text}");
    assert!(lines[1].ends_with("big_2.png  21-25"), "{text}");
}
