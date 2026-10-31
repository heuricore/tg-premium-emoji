#[test]
fn versions_agree() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = std::fs::read_to_string(root.join("VERSION")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".claude-plugin/plugin.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(file.trim(), env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
}
