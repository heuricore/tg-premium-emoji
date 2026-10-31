use std::path::{Path, PathBuf};

use crate::cli::Cli;

pub const HOME_ENV: &str = "TG_PREMIUM_EMOJI_HOME";
pub const BUILTIN_ENV: &str = "TGE_BUILTIN";
pub const PROJECT_DIR: &str = ".tg-premium-emoji";

#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    pub builtin: Option<PathBuf>,
    pub cwd: PathBuf,
    pub catalog_dir: Option<PathBuf>,
}

impl Env {
    pub fn from_cli(cli: &Cli) -> Env {
        let home = cli
            .home
            .clone()
            .or_else(|| env_path(HOME_ENV))
            .unwrap_or_else(|| {
                std::env::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(PROJECT_DIR)
            });
        let builtin = cli
            .builtin
            .clone()
            .or_else(|| env_path(BUILTIN_ENV))
            .or_else(exe_builtin);
        Env {
            home,
            builtin,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            catalog_dir: cli.catalog_dir.clone(),
        }
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn exe_builtin() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.join("catalog");
    dir.join("catalog.json").is_file().then_some(dir)
}

/// First ancestor of `start` (inclusive) that holds `.tg-premium-emoji/catalog.json`.
pub fn find_project_catalog(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(PROJECT_DIR))
        .find(|dir| dir.join("catalog.json").is_file())
}
