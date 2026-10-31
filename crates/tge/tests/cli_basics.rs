#[test]
fn version_flag() {
    assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout("tge 0.0.1\n");
}

#[test]
fn unknown_command_is_usage_error() {
    assert_cmd::Command::cargo_bin("tge")
        .unwrap()
        .arg("frobnicate")
        .assert()
        .code(2);
}
