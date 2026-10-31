use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tge::schema::DocKind;
use tge::validate::{Severity, validate_catalog_dir, validate_value};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalog-ok")
}

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

fn catalog_copy() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixture(), tmp.path());
    tmp
}

fn edit_pack(dir: &Path, f: impl FnOnce(&mut Value)) {
    let path = dir.join("packs/demo/pack.json");
    let mut pack: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    f(&mut pack);
    fs::write(path, serde_json::to_string_pretty(&pack).unwrap()).unwrap();
}

fn errors(dir: &Path) -> Vec<String> {
    validate_catalog_dir(dir)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.message)
        .collect()
}

#[test]
fn ok_catalog_has_no_issues() {
    assert_eq!(validate_catalog_dir(&fixture()), vec![]);
}

#[test]
fn duplicate_id_is_error() {
    let tmp = catalog_copy();
    edit_pack(tmp.path(), |p| {
        p["emoji"][1]["id"] = p["emoji"][0]["id"].clone();
    });
    assert!(
        errors(tmp.path())
            .iter()
            .any(|m| m.contains("duplicate id"))
    );
}

#[test]
fn gap_in_n_is_error() {
    let tmp = catalog_copy();
    edit_pack(tmp.path(), |p| p["emoji"][2]["n"] = 5.into());
    assert!(
        errors(tmp.path())
            .iter()
            .any(|m| m.contains("without gaps"))
    );
}

#[test]
fn gone_with_number_is_error() {
    let tmp = catalog_copy();
    edit_pack(tmp.path(), |p| p["emoji"][2]["gone"] = true.into());
    assert!(
        errors(tmp.path())
            .iter()
            .any(|m| m.contains("is gone but has n"))
    );
}

#[test]
fn missing_sheet_is_error() {
    let tmp = catalog_copy();
    fs::remove_file(tmp.path().join("packs/demo/preview/demo_1.png")).unwrap();
    assert!(
        errors(tmp.path())
            .iter()
            .any(|m| m.contains("preview sheet is missing"))
    );
}

#[test]
fn description_over_300_is_schema_error() {
    let tmp = catalog_copy();
    edit_pack(tmp.path(), |p| {
        p["emoji"][0]["description"] = "я".repeat(301).into();
    });
    assert!(!errors(tmp.path()).is_empty());
    let ok = fs::read_to_string(fixture().join("packs/demo/pack.json")).unwrap();
    let mut value: Value = serde_json::from_str(&ok).unwrap();
    assert!(validate_value(DocKind::Pack, &value).is_empty());
    value["emoji"][0]["description"] = "я".repeat(300).into();
    assert!(validate_value(DocKind::Pack, &value).is_empty());
    value["emoji"][0]["description"] = "я".repeat(301).into();
    assert!(!validate_value(DocKind::Pack, &value).is_empty());
}

#[test]
fn pack_json_over_2_mib_is_error() {
    let tmp = catalog_copy();
    edit_pack(tmp.path(), |p| {
        p["title"] = "x".repeat(2 * 1024 * 1024).into();
    });
    assert!(errors(tmp.path()).iter().any(|m| m.contains("limit is")));
}

#[test]
fn schema_version_2_is_error() {
    let tmp = catalog_copy();
    edit_pack(tmp.path(), |p| p["schema_version"] = 2.into());
    assert!(
        errors(tmp.path())
            .iter()
            .any(|m| m.contains("unsupported version 2"))
    );
}

#[test]
fn cli_validate_exit_codes() {
    let tmp = catalog_copy();
    assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .arg("validate")
        .arg(tmp.path())
        .assert()
        .success();

    edit_pack(tmp.path(), |p| {
        p["emoji"][1]["id"] = p["emoji"][0]["id"].clone();
    });
    let out = assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .arg("validate")
        .arg(tmp.path())
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(out).unwrap().contains("duplicate id"));

    let out = assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .args(["--json", "validate"])
        .arg(tmp.path())
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let doc: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(doc["errors"], 1);
    assert_eq!(doc["issues"][0]["severity"], "error");
}
