#[test]
fn version_flag() {
    assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("tge {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn json_usage_error_is_a_json_document() {
    let out = assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .args(["--json", "frobnicate"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let doc: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(doc["error"].is_string());
}

#[test]
fn unknown_command_is_usage_error() {
    assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .arg("frobnicate")
        .assert()
        .code(2);
}
