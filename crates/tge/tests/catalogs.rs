use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tge::paths::{Env, find_project_catalog};
use tge::store::catalogs::{Catalogs, Role, WriteTarget};
use tge::validate::validate_catalog_dir;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalog-ok")
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn catalog_json(id: &str) -> String {
    format!(
        r#"{{"schema_version":1,"id":"{id}","name":"{id}","version":"0.1.0","author":"t","license":"MIT","description":"d"}}"#
    )
}

fn pack_json(short_name: &str) -> String {
    format!(
        r#"{{"schema_version":1,"short_name":"{short_name}","title":"T","fetched_at":"2026-01-01T00:00:00Z","preview":{{"cell":144,"cols":5,"per_sheet":20}},"emoji":[]}}"#
    )
}

fn make_catalog(dir: &Path, id: &str, packs: &[&str]) {
    write(&dir.join("catalog.json"), &catalog_json(id));
    for p in packs {
        write(&dir.join("packs").join(p).join("pack.json"), &pack_json(p));
    }
}

fn env_in(tmp: &Path) -> Env {
    Env {
        home: tmp.join("home"),
        builtin: None,
        cwd: tmp.join("work"),
        catalog_dir: None,
    }
}

fn tge(tmp: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("tge").unwrap();
    cmd.env_remove("TGE_BUILTIN")
        .env_remove("TG_PREMIUM_EMOJI_HOME")
        .env_remove("BOT_TOKEN")
        .current_dir(tmp);
    cmd
}

fn json_of(out: &[u8]) -> Value {
    serde_json::from_slice(out).unwrap()
}

#[test]
fn home_from_env_var() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("state");
    let out = tge(tmp.path())
        .env("TG_PREMIUM_EMOJI_HOME", &home)
        .args(["--json", "doctor"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        json_of(&out)["home"],
        Value::String(home.display().to_string())
    );
}

#[test]
fn builtin_from_flag_and_exe_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let flag_dir = tmp.path().join("flagged");
    make_catalog(&flag_dir, "flagged", &[]);
    let out = tge(tmp.path())
        .args(["--json", "--home"])
        .arg(&home)
        .arg("--builtin")
        .arg(&flag_dir)
        .arg("doctor")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        json_of(&out)["builtin"],
        Value::String(flag_dir.display().to_string())
    );

    let bin_dir = tmp.path().join("install");
    fs::create_dir_all(&bin_dir).unwrap();
    let exe = bin_dir.join(if cfg!(windows) { "tge.exe" } else { "tge" });
    fs::copy(assert_cmd::cargo::cargo_bin("tge"), &exe).unwrap();
    make_catalog(&bin_dir.join("catalog"), "beside-exe", &[]);
    let out = std::process::Command::new(&exe)
        .args(["--json", "--home"])
        .arg(&home)
        .arg("doctor")
        .env_remove("TGE_BUILTIN")
        .output()
        .unwrap()
        .stdout;
    assert_eq!(
        json_of(&out)["builtin"],
        Value::String(bin_dir.join("catalog").display().to_string())
    );
}

#[test]
fn project_catalog_found_from_nested_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("proj");
    make_catalog(&project.join(".tg-premium-emoji"), "project", &[]);
    let nested = project.join("a/b/c");
    fs::create_dir_all(&nested).unwrap();
    assert_eq!(
        find_project_catalog(&nested),
        Some(project.join(".tg-premium-emoji"))
    );
    assert_eq!(find_project_catalog(tmp.path()), None);
}

#[test]
fn priority_order() {
    let tmp = tempfile::tempdir().unwrap();
    let mut env = env_in(tmp.path());
    let explicit = tmp.path().join("explicit");
    make_catalog(&explicit, "explicit", &[]);
    make_catalog(&tmp.path().join("work/.tg-premium-emoji"), "project", &[]);
    make_catalog(&env.home.join("catalogs/local"), "local", &[]);
    let builtin = tmp.path().join("builtin");
    make_catalog(&builtin, "builtin", &[]);
    env.catalog_dir = Some(explicit);
    env.builtin = Some(builtin);
    let (catalogs, issues) = Catalogs::discover(&env);
    assert!(issues.is_empty(), "{issues:?}");
    let roles: Vec<Role> = catalogs.catalogs.iter().map(|c| c.role).collect();
    assert_eq!(
        roles,
        vec![Role::Explicit, Role::Project, Role::Local, Role::Builtin]
    );
}

#[test]
fn pack_lookup_is_case_insensitive() {
    let tmp = tempfile::tempdir().unwrap();
    let mut env = env_in(tmp.path());
    make_catalog(
        &env.home.join("catalogs/local"),
        "local",
        &["tgiosicons", "mine"],
    );
    let builtin = tmp.path().join("builtin");
    make_catalog(&builtin, "builtin", &["tgiosicons", "other"]);
    env.builtin = Some(builtin);
    let (catalogs, issues) = Catalogs::discover(&env);
    assert!(issues.is_empty(), "{issues:?}");

    let (catalog, pack) = catalogs.find_pack("TgIOSicons").unwrap();
    assert_eq!(catalog.role, Role::Local);
    assert_eq!(pack.short_name, "tgiosicons");
    assert!(catalogs.find_pack("missing").is_none());

    let visible: Vec<(Role, String)> = catalogs
        .visible_packs()
        .into_iter()
        .map(|(c, p)| (c.role, p.short_name))
        .collect();
    assert_eq!(visible.iter().filter(|(_, n)| n == "tgiosicons").count(), 1);
    assert!(visible.contains(&(Role::Builtin, "other".to_string())));
    assert_eq!(visible.len(), 3);
}

#[test]
fn invalid_catalog_skipped_with_issue() {
    let tmp = tempfile::tempdir().unwrap();
    let mut env = env_in(tmp.path());
    let local = env.home.join("catalogs/local");
    write(
        &local.join("catalog.json"),
        r#"{"schema_version":1,"id":"BAD ID"}"#,
    );
    let builtin = tmp.path().join("builtin");
    make_catalog(&builtin, "builtin", &["p"]);
    env.builtin = Some(builtin);
    let (catalogs, issues) = Catalogs::discover(&env);
    assert_eq!(catalogs.catalogs.len(), 1);
    assert_eq!(catalogs.catalogs[0].role, Role::Builtin);
    assert!(!issues.is_empty());
    assert!(issues.iter().all(|i| i.file.starts_with(&local)));
}

#[test]
fn project_target_created_in_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    fs::create_dir_all(&env.cwd).unwrap();
    let dir = Catalogs::writable_dir(&env, WriteTarget::Project).unwrap();
    assert_eq!(dir, env.cwd.join(".tg-premium-emoji"));
    let meta: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("catalog.json")).unwrap()).unwrap();
    assert_eq!(meta["id"], "project");
    assert!(
        validate_catalog_dir(&dir)
            .iter()
            .all(|i| i.file.ends_with("packs")),
        "generated catalog.json must be valid"
    );

    let local = Catalogs::writable_dir(&env, WriteTarget::Local).unwrap();
    assert_eq!(local, env.home.join("catalogs/local"));
    let meta: Value =
        serde_json::from_str(&fs::read_to_string(local.join("catalog.json")).unwrap()).unwrap();
    assert_eq!(meta["id"], "local");
}

#[test]
fn builtin_never_writable() {
    let tmp = tempfile::tempdir().unwrap();
    let mut env = env_in(tmp.path());
    fs::create_dir_all(&env.cwd).unwrap();
    let builtin = tmp.path().join("builtin");
    make_catalog(&builtin, "builtin", &[]);
    env.builtin = Some(builtin.clone());
    for target in [WriteTarget::Local, WriteTarget::Project] {
        let dir = Catalogs::writable_dir(&env, target).unwrap();
        assert_ne!(dir, builtin);
    }
    env.catalog_dir = Some(builtin);
    assert!(Catalogs::writable_dir(&env, WriteTarget::Explicit).is_err());
    env.catalog_dir = None;
    assert!(Catalogs::writable_dir(&env, WriteTarget::Explicit).is_err());
}

#[test]
fn catalogs_command_lists_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    make_catalog(&home.join("catalogs/local"), "local", &["a", "b"]);
    let out = tge(tmp.path())
        .args(["--json", "--home"])
        .arg(&home)
        .arg("--builtin")
        .arg(fixture())
        .arg("catalogs")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc = json_of(&out);
    let rows = doc["catalogs"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["role"], "local");
    assert_eq!(rows[0]["id"], "local");
    assert_eq!(rows[0]["packs"], 2);
    assert_eq!(rows[1]["role"], "builtin");
    assert_eq!(rows[1]["id"], "demo");
    assert!(doc["issues"].as_array().unwrap().is_empty());

    let text = tge(tmp.path())
        .args(["--home"])
        .arg(&home)
        .arg("--builtin")
        .arg(fixture())
        .arg("catalogs")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(text).unwrap();
    assert!(
        text.lines()
            .any(|l| l.starts_with("local local ") && l.ends_with(" 2 packs"))
    );
}

#[test]
fn validate_without_path_checks_discovered_catalogs() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    tge(tmp.path())
        .args(["--home"])
        .arg(&home)
        .arg("--builtin")
        .arg(fixture())
        .arg("validate")
        .assert()
        .success();
    let local = home.join("catalogs/local");
    make_catalog(&local, "local", &[]);
    write(&local.join("packs/broken/pack.json"), "{}");
    tge(tmp.path())
        .args(["--home"])
        .arg(&home)
        .arg("--builtin")
        .arg(fixture())
        .arg("validate")
        .assert()
        .code(3);
}

#[test]
fn doctor_never_prints_token() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tge(tmp.path())
        .env("BOT_TOKEN", "123:SECRET")
        .args(["--json", "--home"])
        .arg(tmp.path().join("home"))
        .arg("doctor")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains("SECRET"));
    assert_eq!(json_of(text.as_bytes())["bot_token"], "set");

    let out = tge(tmp.path())
        .args(["--json", "--home"])
        .arg(tmp.path().join("home"))
        .arg("doctor")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(json_of(&out)["bot_token"], "not set");
}

#[cfg(unix)]
#[test]
fn prune_keeps_latest() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    for v in ["0.1.0", "0.2.0", "0.3.0"] {
        write(&home.join("bin").join(v).join("tge"), "x");
    }
    std::os::unix::fs::symlink("0.2.0", home.join("bin/latest")).unwrap();
    tge(tmp.path())
        .args(["--json", "--home"])
        .arg(&home)
        .args(["doctor", "--prune"])
        .assert()
        .success();
    assert!(home.join("bin/0.2.0/tge").exists());
    assert!(!home.join("bin/0.1.0").exists());
    assert!(!home.join("bin/0.3.0").exists());
    assert!(home.join("bin/latest").exists());
}
