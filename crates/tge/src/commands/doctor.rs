use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::commands::catalogs::issue_line;
use crate::error::Error;
use crate::output::Ctx;
use crate::store::catalogs::Catalogs;

pub fn run(ctx: &Ctx, prune: bool) -> Result<(), Error> {
    let env = &ctx.env;
    let (catalogs, issues) = Catalogs::discover(env);
    let bot_token = if std::env::var_os("BOT_TOKEN").is_some_and(|v| !v.is_empty()) {
        "set"
    } else {
        "not set"
    };
    let writable = dir_writable(&env.home);
    let pruned = if prune {
        prune_bin(&env.home.join("bin"))?
    } else {
        Vec::new()
    };

    let mut lines = vec![
        format!("version: {}", env!("CARGO_PKG_VERSION")),
        format!("home: {}", env.home.display()),
        format!(
            "builtin: {}",
            env.builtin
                .as_ref()
                .map_or("missing".to_string(), |p| p.display().to_string())
        ),
        format!("bot_token: {bot_token}"),
        format!("home writable: {}", if writable { "yes" } else { "no" }),
        format!("catalogs: {}", catalogs.catalogs.len()),
    ];
    lines.extend(catalogs.catalogs.iter().map(|c| {
        format!(
            "  {} {} {} ({} packs)",
            c.role.label(),
            c.meta.id,
            c.dir.display(),
            c.packs().len()
        )
    }));
    lines.extend(issues.iter().map(issue_line));
    lines.extend(pruned.iter().map(|p| format!("pruned: {}", p.display())));

    let catalog_rows: Vec<_> = catalogs
        .catalogs
        .iter()
        .map(|c| json!({ "role": c.role, "id": c.meta.id, "dir": c.dir, "packs": c.packs().len() }))
        .collect();
    ctx.emit(
        &lines.join("\n"),
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "home": env.home,
            "builtin": env.builtin,
            "catalogs": catalog_rows,
            "issues": issues,
            "bot_token": bot_token,
            "home_writable": writable,
            "pruned": pruned,
        }),
    );
    Ok(())
}

/// Probes the home directory, or its nearest existing ancestor when it is not created yet.
fn dir_writable(home: &Path) -> bool {
    let Some(dir) = home.ancestors().find(|p| p.is_dir()) else {
        return false;
    };
    let probe = dir.join(format!(".tge-probe-{}", std::process::id()));
    let ok = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

/// Removes `bin/<version>` folders except the target of `bin/latest` and the running executable's.
fn prune_bin(bin: &Path) -> Result<Vec<PathBuf>, Error> {
    let Ok(entries) = fs::read_dir(bin) else {
        return Ok(Vec::new());
    };
    let latest = fs::canonicalize(bin.join("latest")).map_err(|_| {
        Error::Runtime(format!(
            "{} is missing or dangling, nothing pruned",
            bin.join("latest").display()
        ))
    })?;
    let running = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let mut removed = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if entry.file_name() == "latest" || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(canonical) = fs::canonicalize(&path) else {
            continue;
        };
        if canonical == latest || running.as_ref().is_some_and(|r| r.starts_with(&canonical)) {
            continue;
        }
        fs::remove_dir_all(&path)?;
        removed.push(path);
    }
    removed.sort();
    Ok(removed)
}
